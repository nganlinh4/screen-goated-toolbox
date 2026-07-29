use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::{
    InspectedImage, MANIFEST_NAME, MAX_MANIFEST_BYTES, PREVIEW_ROOT_NAME, ROOT_NAME, SNAPSHOT_LOCK,
    SnapshotManifest,
};

pub(super) fn snapshot_root() -> Result<PathBuf, String> {
    managed_root(ROOT_NAME)
}

pub(super) fn preview_root() -> Result<PathBuf, String> {
    managed_root(PREVIEW_ROOT_NAME)
}

fn managed_root(name: &str) -> Result<PathBuf, String> {
    let root = crate::paths::app_runtime_local_data_dir().join(name);
    std::fs::create_dir_all(&root)
        .map_err(|_| "Creation source storage is unavailable.".to_string())?;
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|_| "Creation source storage is unavailable.".to_string())?;
    if !metadata.file_type().is_dir() || is_reparse_point(&metadata) {
        return Err("Creation source storage is unavailable.".to_string());
    }
    std::fs::canonicalize(root).map_err(|_| "Creation source storage is unavailable.".to_string())
}

pub(super) fn validate_snapshot_directory(
    root: &Path,
    snapshot_id: &str,
) -> Result<PathBuf, String> {
    if !super::valid_id(snapshot_id) {
        return Err("Creation source assignment is invalid.".to_string());
    }
    let directory = root.join(snapshot_id);
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "Saved creation source state is unavailable.".to_string())?;
    if !metadata.file_type().is_dir() || is_reparse_point(&metadata) {
        return Err("Saved creation source state is invalid.".to_string());
    }
    let canonical = std::fs::canonicalize(&directory)
        .map_err(|_| "Saved creation source state is unavailable.".to_string())?;
    if canonical.parent() != Some(root)
        || canonical
            .file_name()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case(snapshot_id))
    {
        return Err("Saved creation source state is invalid.".to_string());
    }
    Ok(canonical)
}

pub(super) fn read_manifest(directory: &Path) -> Result<SnapshotManifest, String> {
    let path = directory.join(MANIFEST_NAME);
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| "Saved creation source state is unavailable.".to_string())?;
    if !metadata.file_type().is_file()
        || is_reparse_point(&metadata)
        || metadata.len() > MAX_MANIFEST_BYTES
    {
        return Err("Saved creation source state is invalid.".to_string());
    }
    let file = std::fs::File::open(path)
        .map_err(|_| "Saved creation source state is unavailable.".to_string())?;
    serde_json::from_reader(file.take(MAX_MANIFEST_BYTES + 1))
        .map_err(|_| "Saved creation source state is invalid.".to_string())
}

pub(super) fn write_manifest(directory: &Path, manifest: &SnapshotManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|_| "Creation source state could not be saved.".to_string())?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Creation source state exceeds its storage limit.".to_string());
    }
    crate::atomic_json::write_json_atomic(&directory.join(MANIFEST_NAME), manifest)
        .map_err(|_| "Creation source state could not be saved.".to_string())
}

pub(super) fn cleanup_snapshot(snapshot_id: &str) -> Result<(), String> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| "Creation source cleanup is unavailable.".to_string())?;
    let root = snapshot_root()?;
    let directory = validate_snapshot_directory(&root, snapshot_id)?;
    match read_manifest(&directory) {
        Ok(manifest) if super::validate_manifest(&root, &manifest).is_ok() => {
            cleanup_manifest_directory(&directory, &manifest)
        }
        Ok(_) => cleanup_uncommitted_directory(&directory),
        Err(_) => cleanup_uncommitted_directory(&directory),
    }
}

pub(super) fn cleanup_manifest_directory(
    directory: &Path,
    manifest: &SnapshotManifest,
) -> Result<(), String> {
    let expected = manifest
        .entries
        .iter()
        .filter_map(|entry| {
            Path::new(&entry.descriptor.path)
                .file_name()
                .map(|name| name.to_os_string())
        })
        .chain([MANIFEST_NAME.into()])
        .collect::<HashSet<_>>();
    for entry in std::fs::read_dir(directory)
        .map_err(|_| "Creation source cleanup is unavailable.".to_string())?
    {
        let entry = entry.map_err(|_| "Creation source cleanup is unavailable.".to_string())?;
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|_| "Creation source cleanup is unavailable.".to_string())?;
        if !metadata.file_type().is_file()
            || is_reparse_point(&metadata)
            || !expected.contains(&entry.file_name())
        {
            return Err("Creation source cleanup found unexpected content.".to_string());
        }
    }
    for entry in &manifest.entries {
        remove_regular_if_present(Path::new(&entry.descriptor.path))?;
    }
    remove_regular_if_present(&directory.join(MANIFEST_NAME))?;
    std::fs::remove_dir(directory)
        .map_err(|_| "Creation source cleanup could not finish.".to_string())
}

pub(super) fn cleanup_uncommitted_directory(directory: &Path) -> Result<(), String> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Creation source cleanup is unavailable.".to_string()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "Creation source cleanup is unavailable.".to_string())?;
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|_| "Creation source cleanup is unavailable.".to_string())?;
        let valid_name = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name == MANIFEST_NAME || name.starts_with("source-"));
        if !metadata.file_type().is_file() || is_reparse_point(&metadata) || !valid_name {
            return Err("Creation source cleanup found unexpected content.".to_string());
        }
        paths.push(entry.path());
    }
    for path in paths {
        remove_regular_if_present(&path)?;
    }
    std::fs::remove_dir(directory)
        .map_err(|_| "Creation source cleanup could not finish.".to_string())
}

fn remove_regular_if_present(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => {
            std::fs::remove_file(path)
                .map_err(|_| "Creation source cleanup could not finish.".to_string())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Creation source cleanup found unexpected content.".to_string()),
    }
}

#[cfg(windows)]
pub(super) fn copy_locked(source: &InspectedImage, target: &Path) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let input = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(&source.path)
        .map_err(|_| "Creation source could not be locked.".to_string())?;
    copy_checked(input, source, target)
}

#[cfg(not(windows))]
pub(super) fn copy_locked(source: &InspectedImage, target: &Path) -> Result<(), String> {
    let input = std::fs::File::open(&source.path)
        .map_err(|_| "Creation source could not be locked.".to_string())?;
    copy_checked(input, source, target)
}

fn copy_checked(
    mut input: std::fs::File,
    source: &InspectedImage,
    target: &Path,
) -> Result<(), String> {
    let metadata = input
        .metadata()
        .map_err(|_| "Creation source could not be inspected.".to_string())?;
    if !metadata.is_file() || metadata.len() != source.size_bytes {
        return Err("Creation source changed before it was accepted.".to_string());
    }
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|_| "Creation source copy could not be created.".to_string())?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|_| "Creation source could not be read.".to_string())?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > source.size_bytes {
            return Err("Creation source changed before it was accepted.".to_string());
        }
        digest.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|_| "Creation source copy could not be saved.".to_string())?;
    }
    output
        .flush()
        .and_then(|_| output.sync_all())
        .map_err(|_| "Creation source copy could not be saved.".to_string())?;
    if total != source.size_bytes || format!("{:x}", digest.finalize()) != source.sha256 {
        return Err("Creation source changed before it was accepted.".to_string());
    }
    Ok(())
}

pub(super) fn file_sha256(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "Creation preview is unavailable.".to_string())?;
    if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
        return Err("Creation preview is unavailable.".to_string());
    }
    let mut file =
        std::fs::File::open(path).map_err(|_| "Creation preview is unavailable.".to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "Creation preview is unavailable.".to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(windows)]
pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
