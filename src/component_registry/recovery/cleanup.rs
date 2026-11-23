use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};
use sha2::{Digest as _, Sha256};

use super::{
    AUTHENTICATED, MAX_ENTRIES, MAX_REASON_CHARS, MAX_RECORD_BYTES, Record, SUFFIX, recovery_root,
};
use crate::component_registry::receipt::{
    is_reparse_point, resolve_owned_path, validate_relative_path,
};

const MAX_DIRECTORIES: usize = 256;

#[derive(Clone, Debug)]
pub(crate) struct RecoveryEntry {
    pub(crate) path: PathBuf,
    pub(crate) reason: String,
    pub(crate) can_clean: bool,
    record_path: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct RecoveryOutcome {
    pub(crate) path: PathBuf,
    pub(crate) preserved_paths: Vec<PathBuf>,
}

pub(crate) fn list() -> Result<Vec<RecoveryEntry>> {
    let root = recovery_root();
    let Some(ids) = directory_entries(&root)? else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for parent in ids {
        let Some(id) = parent.file_name().and_then(|name| name.to_str()) else {
            result.push(unknown(parent));
            continue;
        };
        if crate::component_registry::catalog::validate_identifier(id).is_err() {
            result.push(unknown(parent));
            continue;
        }
        let Some(entries) = directory_entries(&parent).ok().flatten() else {
            result.push(unknown(parent));
            continue;
        };
        let mut represented = HashSet::new();
        for sidecar in &entries {
            let Some(target) = sidecar_target(sidecar) else {
                continue;
            };
            represented.insert(target.clone());
            match read_record(sidecar, id) {
                Ok((record, hash))
                    if target.file_name().and_then(|name| name.to_str())
                        == Some(record.directory.as_str())
                        && authenticated(sidecar, &hash.1) =>
                {
                    result.push(RecoveryEntry {
                        path: target,
                        reason: record.reason,
                        can_clean: true,
                        record_path: Some(sidecar.clone()),
                    });
                }
                Ok((record, _)) => result.push(RecoveryEntry {
                    path: target,
                    reason: format!(
                        "{} Recovery ownership could not be verified; files are preserved.",
                        record.reason
                    ),
                    can_clean: false,
                    record_path: None,
                }),
                _ => result.push(unknown(sidecar.clone())),
            }
        }
        for entry in entries {
            if sidecar_target(&entry).is_none() && !represented.contains(&entry) {
                result.push(unknown(entry));
            }
        }
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

pub(crate) fn clean(entry: &RecoveryEntry) -> Result<RecoveryOutcome> {
    let _mutation = crate::component_registry::acquire_mutation_guard()?;
    clean_inner(entry, false)
}

#[cfg(test)]
pub(super) fn purge(entry: &RecoveryEntry) -> Result<RecoveryOutcome> {
    let _mutation = crate::component_registry::acquire_mutation_guard()?;
    clean_inner(entry, true)
}

pub(crate) fn purge_all_recorded() -> Result<Vec<RecoveryOutcome>> {
    let _mutation = crate::component_registry::acquire_mutation_guard()?;
    list()?
        .iter()
        .map(|entry| clean_inner(entry, true))
        .collect()
}

fn clean_inner(entry: &RecoveryEntry, purge_changed: bool) -> Result<RecoveryOutcome> {
    let Some(sidecar) = &entry.record_path else {
        return Ok(preserved(entry.path.clone()));
    };
    let Some(parent) = sidecar.parent() else {
        bail!("component recovery has no parent")
    };
    if parent.parent() != Some(recovery_root().as_path())
        || sidecar_target(sidecar).as_deref() != Some(entry.path.as_path())
    {
        bail!("component recovery path escaped its root");
    }
    let id = parent
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("component recovery id is invalid"))?;
    let (record, record_hash) = read_record(sidecar, id)?;
    if entry.path.file_name().and_then(|name| name.to_str()) != Some(record.directory.as_str())
        || !authenticated(sidecar, &record_hash.1)
    {
        bail!("component recovery sidecar does not match its directory");
    }
    let metadata = match std::fs::symlink_metadata(&entry.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(preserved(sidecar.clone()));
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Ok(preserved(entry.path.clone()));
    }
    let mut preserved_paths = Vec::new();
    let mut directories = Vec::new();
    for file in &record.files {
        let path = match resolve_owned_path(&entry.path, &file.path) {
            Ok(path) => path,
            Err(_) => {
                preserved_paths.push(entry.path.join(&file.path));
                continue;
            }
        };
        if file.directory {
            directories.push(path);
            continue;
        }
        if !(file.owned || (purge_changed && file.declared)) || file.sha256.is_none() {
            preserved_paths.push(path);
            continue;
        }
        match delete_regular(
            &path,
            (!purge_changed)
                .then_some((file.size_bytes, file.sha256.as_deref().unwrap_or_default())),
        ) {
            Ok(true) => remove_empty_parents(path.parent(), &entry.path),
            Ok(false) | Err(_) => preserved_paths.push(path),
        }
    }
    for directory in directories {
        if std::fs::symlink_metadata(&directory).is_ok() {
            preserved_paths.push(directory);
        }
    }
    let _ = delete_empty_directory(&entry.path);
    match std::fs::symlink_metadata(&entry.path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !delete_regular(sidecar, Some((record_hash.0, &record_hash.1)))? {
                preserved_paths.push(sidecar.clone());
            }
        }
        _ => preserved_paths.push(entry.path.clone()),
    }
    Ok(RecoveryOutcome {
        path: entry.path.clone(),
        preserved_paths,
    })
}

fn read_record(path: &Path, expected_id: &str) -> Result<(Record, (u64, String))> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > MAX_RECORD_BYTES {
        bail!("component recovery record is unsafe");
    }
    let mut file = std::fs::File::open(path)?;
    let mut body = Vec::new();
    file.by_ref()
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut body)?;
    if body.len() as u64 != metadata.len() {
        bail!("component recovery record changed");
    }
    let record: Record = serde_json::from_slice(&body)?;
    crate::component_registry::component_version_root(&record.id, &record.version)?;
    if record.schema_version != 1
        || record.id != expected_id
        || record.reason.chars().count() > MAX_REASON_CHARS
        || record.files.len() > MAX_ENTRIES
    {
        bail!("component recovery record is invalid");
    }
    let directory = Path::new(&record.directory);
    if directory.components().count() != 1
        || !matches!(
            directory.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        bail!("component recovery directory is invalid");
    }
    let mut paths = HashSet::new();
    for file in &record.files {
        validate_relative_path(&file.path)?;
        if !paths.insert(&file.path)
            || file.sha256.as_ref().is_some_and(|hash| {
                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            || (file.owned && file.sha256.is_none())
            || (file.directory && (file.owned || file.sha256.is_some()))
        {
            bail!("component recovery inventory is invalid");
        }
    }
    Ok((
        record,
        (body.len() as u64, format!("{:x}", Sha256::digest(&body))),
    ))
}

fn directory_entries(path: &Path) -> Result<Option<Vec<PathBuf>>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component recovery directory is unsafe");
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        if entries.len() >= MAX_DIRECTORIES {
            bail!("component recovery directory has too many entries");
        }
        entries.push(entry?.path());
    }
    Ok(Some(entries))
}

fn sidecar_target(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?.strip_suffix(SUFFIX)?;
    if name.is_empty() {
        return None;
    }
    Some(path.with_file_name(name))
}

fn authenticated(path: &Path, hash: &str) -> bool {
    AUTHENTICATED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(path)
        .is_some_and(|expected| expected == hash)
}

fn unknown(path: PathBuf) -> RecoveryEntry {
    RecoveryEntry {
        path,
        reason: "Recovery metadata is missing or invalid; files are preserved.".into(),
        can_clean: false,
        record_path: None,
    }
}

fn preserved(path: PathBuf) -> RecoveryOutcome {
    RecoveryOutcome {
        path: path.clone(),
        preserved_paths: vec![path],
    }
}

pub(crate) fn remove_empty_parents(mut parent: Option<&Path>, root: &Path) {
    while let Some(directory) = parent {
        if directory == root || delete_empty_directory(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}

pub(crate) fn delete_regular(path: &Path, expected: Option<(u64, &str)>) -> Result<bool> {
    delete_regular_cancellable(path, expected, &AtomicBool::new(false))
}

pub(crate) fn delete_regular_cancellable(
    path: &Path,
    expected: Option<(u64, &str)>,
    cancelled: &AtomicBool,
) -> Result<bool> {
    check_cancelled(cancelled)?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {}
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    }
    let _ancestors = lock_ancestors(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
        };
        options
            .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Ok(false);
    }
    if let Some((size, hash)) = expected {
        if metadata.len() != size {
            return Ok(false);
        }
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            check_cancelled(cancelled)?;
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        if !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(hash) {
            return Ok(false);
        }
    }
    check_cancelled(cancelled)?;
    #[cfg(windows)]
    mark_for_deletion(&file)?;
    #[cfg(not(windows))]
    std::fs::remove_file(path)?;
    Ok(true)
}

pub(crate) fn delete_empty_directory(path: &Path) -> Result<()> {
    let _ancestors = lock_ancestors(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
            FILE_SHARE_READ,
        };
        let file = std::fs::OpenOptions::new()
            .read(true)
            .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(path)?;
        delete_empty_directory_handle(&file)?;
    }
    #[cfg(not(windows))]
    std::fs::remove_dir(path)?;
    Ok(())
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        bail!("component cleanup cancelled");
    }
    Ok(())
}

fn delete_empty_directory_handle(file: &std::fs::File) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component recovery directory is unsafe");
    }
    mark_for_deletion(file)
}

#[cfg(windows)]
fn lock_ancestors(path: &Path) -> Result<Vec<std::fs::File>> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("recovery path has no parent"))?;
    if !parent.is_absolute() {
        bail!("recovery path is not absolute");
    }
    let mut paths = parent.ancestors().collect::<Vec<_>>();
    paths.reverse();
    let mut guards = Vec::with_capacity(paths.len());
    for directory in paths {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .access_mode(FILE_GENERIC_READ.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(directory)?;
        let metadata = file.metadata()?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            bail!("recovery path contains an unsafe directory");
        }
        guards.push(file);
    }
    Ok(guards)
}

#[cfg(not(windows))]
fn lock_ancestors(_path: &Path) -> Result<Vec<std::fs::File>> {
    bail!("safe recovery cleanup is unavailable on this platform")
}

#[cfg(windows)]
fn mark_for_deletion(file: &std::fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the handle is live and the structure has the required size.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )?
    };
    Ok(())
}
