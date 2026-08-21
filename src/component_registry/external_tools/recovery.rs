use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    ExternalTool, ExternalToolDelivery, delivery_optional, recovery_io, staging, version_root,
};
use crate::component_registry::receipt::{
    RECEIPT_NAME, is_reparse_point, resolve_owned_path, validate_relative_path,
};

const RECORD_SUFFIX: &str = ".recovery.json";
const TEMP_SUFFIX: &str = ".tmp";
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const MAX_RECEIPT_SNAPSHOT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REASON_CHARS: usize = 2048;
const MAX_RECOVERY_ENTRIES: usize = 128;
const MAX_PRESERVED_PATHS: usize = 128;
static RECOVERY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct ExternalToolRecovery {
    pub(crate) path: PathBuf,
    pub(crate) reason: String,
    pub(crate) can_clean: bool,
    record_path: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct RecoveryCleanupOutcome {
    pub(crate) path: PathBuf,
    pub(crate) removed_files: usize,
    pub(crate) preserved_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryRecord {
    schema_version: u32,
    id: String,
    version: String,
    reason: String,
    files: Vec<RecoveryFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryFile {
    path: PathBuf,
    size_bytes: u64,
    sha256: String,
}

struct LoadedRecord {
    value: RecoveryRecord,
    record_file: RecoveryFile,
}

pub(super) fn quarantine_invalid(
    delivery: &ExternalToolDelivery,
    failure_reason: &str,
) -> Result<Option<PathBuf>> {
    quarantine_with(delivery, failure_reason, |from, to| {
        std::fs::rename(from, to)
    })
}

fn quarantine_with(
    delivery: &ExternalToolDelivery,
    failure_reason: &str,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<Option<PathBuf>> {
    let _exclusive = crate::component_registry::lease::reserve_exclusive_mutation(delivery.id)?;
    let source = version_root(delivery)?;
    let metadata = match std::fs::symlink_metadata(&source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("{} invalid version root is unsafe", delivery.id);
    }

    let root = recovery_root();
    let parent = root.join(delivery.id);
    staging::ensure_directory_tree(&root, &parent)?;
    let record = snapshot_record(delivery, &source, failure_reason)?;
    let (target, record_path, record_file) = create_sidecar(&parent, delivery, &record)?;
    if let Err(error) = rename(&source, &target) {
        let rollback =
            recovery_io::delete_if_exact(&record_path, record_file.size_bytes, &record_file.sha256);
        if !matches!(rollback, Ok(true)) {
            return Err(anyhow!(
                "{} recovery move failed and its visible sidecar was preserved at '{}': {error}",
                delivery.id,
                record_path.display()
            ));
        }
        return Err(error.into());
    }
    crate::log_info!(
        "[Components] preserved invalid {} bytes at {}",
        delivery.id,
        target.display()
    );
    Ok(Some(target))
}

pub(crate) fn list(tool: ExternalTool) -> Result<Vec<ExternalToolRecovery>> {
    list_for(tool.id(), delivery_optional(tool))
}

fn list_for(
    id: &str,
    delivery: Option<&ExternalToolDelivery>,
) -> Result<Vec<ExternalToolRecovery>> {
    let parent = recovery_parent(id);
    let entries = recovery_io::bounded_entries(&parent, MAX_RECOVERY_ENTRIES)?;
    let mut recoveries = Vec::new();
    let mut represented = HashSet::new();

    for path in &entries.paths {
        let Some(target) = sidecar_target(path) else {
            continue;
        };
        represented.insert(target.clone());
        let loaded = delivery.and_then(|delivery| read_record(path, delivery).ok());
        match loaded {
            Some(loaded) => recoveries.push(ExternalToolRecovery {
                path: target,
                reason: loaded.value.reason,
                can_clean: true,
                record_path: Some(path.clone()),
            }),
            None => recoveries.push(ExternalToolRecovery {
                path: target,
                reason: "Recovery metadata is missing, invalid, or belongs to another pinned delivery; automatic cleanup is disabled.".to_string(),
                can_clean: false,
                record_path: None,
            }),
        }
    }

    for path in entries.paths {
        if sidecar_target(&path).is_some() || represented.contains(&path) {
            continue;
        }
        recoveries.push(ExternalToolRecovery {
            path,
            reason: "Recovery metadata is missing; this path is preserved and automatic cleanup is disabled."
                .to_string(),
            can_clean: false,
            record_path: None,
        });
    }
    if entries.overflowed {
        recoveries.push(ExternalToolRecovery {
            path: parent,
            reason: "Additional recovery paths exceed the bounded inventory; the entire recovery folder is preserved."
                .to_string(),
            can_clean: false,
            record_path: None,
        });
    }
    recoveries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(recoveries)
}

pub(crate) fn clean(
    tool: ExternalTool,
    recovery: &ExternalToolRecovery,
) -> Result<RecoveryCleanupOutcome> {
    let Some(delivery) = delivery_optional(tool) else {
        return Ok(preserved_outcome(recovery.path.clone()));
    };
    clean_for(delivery, recovery)
}

fn clean_for(
    delivery: &ExternalToolDelivery,
    recovery: &ExternalToolRecovery,
) -> Result<RecoveryCleanupOutcome> {
    let Some(record_path) = recovery.record_path.as_deref() else {
        return Ok(preserved_outcome(recovery.path.clone()));
    };
    let expected_parent = recovery_parent(delivery.id);
    if record_path.parent() != Some(expected_parent.as_path())
        || sidecar_target(record_path).as_deref() != Some(recovery.path.as_path())
    {
        bail!("external tool recovery path escaped its owned directory");
    }
    let loaded = read_record(record_path, delivery)?;
    clean_loaded(&recovery.path, record_path, loaded)
}

pub(crate) fn purge_all_recorded() -> Result<Vec<RecoveryCleanupOutcome>> {
    let mut outcomes = Vec::new();
    for tool in ExternalTool::ALL {
        let recoveries = match list(tool) {
            Ok(recoveries) => recoveries,
            Err(error) => {
                let path = recovery_parent(tool.id());
                crate::log_info!(
                    "[Components] could not list {} recoveries during Clean All: {error:#}",
                    tool.id()
                );
                outcomes.push(preserved_outcome(path));
                continue;
            }
        };
        for recovery in recoveries {
            let Some(delivery) = delivery_optional(tool) else {
                outcomes.push(preserved_outcome(recovery.path));
                continue;
            };
            let Some(record_path) = recovery.record_path.as_deref() else {
                outcomes.push(preserved_outcome(recovery.path));
                continue;
            };
            let result = read_record(record_path, delivery)
                .and_then(|loaded| purge_loaded(&recovery.path, record_path, loaded));
            match result {
                Ok(outcome) => outcomes.push(outcome),
                Err(error) => {
                    crate::log_info!(
                        "[Components] could not clean {} recovery {}: {error:#}",
                        tool.id(),
                        recovery.path.display()
                    );
                    outcomes.push(preserved_outcome(recovery.path));
                }
            }
        }
    }
    Ok(outcomes)
}

fn purge_loaded(
    target: &Path,
    record_path: &Path,
    loaded: LoadedRecord,
) -> Result<RecoveryCleanupOutcome> {
    let mut removed_files = 0;
    let mut preserved = Vec::new();
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {
            for snapshot in &loaded.value.files {
                let path = resolve_owned_path(target, &snapshot.path)?;
                match std::fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {
                        std::fs::remove_file(&path)?;
                        removed_files += 1;
                        recovery_io::remove_empty_parents(path.parent(), target);
                    }
                    Ok(_) => push_preserved(&mut preserved, path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            recovery_io::collect_remaining(target, &mut preserved, MAX_PRESERVED_PATHS, 32);
            if preserved.is_empty() {
                let _ = std::fs::remove_dir(target);
            }
        }
        Ok(_) => push_preserved(&mut preserved, target.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if preserved.is_empty()
        && !recovery_io::delete_if_exact(
            record_path,
            loaded.record_file.size_bytes,
            &loaded.record_file.sha256,
        )?
    {
        push_preserved(&mut preserved, record_path.to_path_buf());
    }
    Ok(RecoveryCleanupOutcome {
        path: target.to_path_buf(),
        removed_files,
        preserved_paths: preserved,
    })
}

fn clean_loaded(
    target: &Path,
    record_path: &Path,
    loaded: LoadedRecord,
) -> Result<RecoveryCleanupOutcome> {
    let mut removed_files = 0;
    let mut preserved = Vec::new();
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {
            for snapshot in &loaded.value.files {
                let path = resolve_owned_path(target, &snapshot.path)?;
                match recovery_io::delete_if_exact(&path, snapshot.size_bytes, &snapshot.sha256) {
                    Ok(true) => {
                        removed_files += 1;
                        recovery_io::remove_empty_parents(path.parent(), target);
                    }
                    Ok(false) | Err(_) => push_preserved(&mut preserved, path),
                }
            }
            recovery_io::collect_remaining(target, &mut preserved, MAX_PRESERVED_PATHS, 32);
            if preserved.is_empty() {
                let _ = std::fs::remove_dir(target);
                if target.exists() {
                    push_preserved(&mut preserved, target.to_path_buf());
                }
            }
        }
        Ok(_) => push_preserved(&mut preserved, target.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if preserved.is_empty() {
        if !recovery_io::delete_if_exact(
            record_path,
            loaded.record_file.size_bytes,
            &loaded.record_file.sha256,
        )? {
            push_preserved(&mut preserved, record_path.to_path_buf());
        } else if let Some(parent) = record_path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    Ok(RecoveryCleanupOutcome {
        path: target.to_path_buf(),
        removed_files,
        preserved_paths: preserved,
    })
}

fn snapshot_record(
    delivery: &ExternalToolDelivery,
    source: &Path,
    failure_reason: &str,
) -> Result<RecoveryRecord> {
    let mut files = Vec::new();
    for expected in delivery.files {
        match snapshot_file(source, Path::new(expected.path), expected.size_bytes) {
            Ok(Some(snapshot)) => files.push(snapshot),
            Ok(None) => {}
            Err(error) => crate::log_info!(
                "[Components] preserving unsnapshotted recovery path {}: {error:#}",
                expected.path
            ),
        }
    }
    match snapshot_file(source, Path::new(RECEIPT_NAME), MAX_RECEIPT_SNAPSHOT_BYTES) {
        Ok(Some(snapshot)) => files.push(snapshot),
        Ok(None) => {}
        Err(error) => {
            crate::log_info!("[Components] preserving unsnapshotted recovery receipt: {error:#}")
        }
    }
    Ok(RecoveryRecord {
        schema_version: 1,
        id: delivery.id.to_string(),
        version: delivery.version.to_string(),
        reason: bounded_reason(failure_reason),
        files,
    })
}

fn snapshot_file(root: &Path, relative: &Path, maximum: u64) -> Result<Option<RecoveryFile>> {
    let path = resolve_owned_path(root, relative)?;
    let Some((size_bytes, sha256)) = recovery_io::snapshot_regular(&path, maximum)? else {
        return Ok(None);
    };
    Ok(Some(RecoveryFile {
        path: relative.to_path_buf(),
        size_bytes,
        sha256,
    }))
}

fn create_sidecar(
    parent: &Path,
    delivery: &ExternalToolDelivery,
    record: &RecoveryRecord,
) -> Result<(PathBuf, PathBuf, RecoveryFile)> {
    let bytes = serde_json::to_vec_pretty(record)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        bail!("external tool recovery metadata is too large");
    }
    for _ in 0..32 {
        let sequence = RECOVERY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let base = format!("{}-{}-{sequence}", delivery.version, std::process::id());
        let target = parent.join(&base);
        let record_path = parent.join(format!("{base}{RECORD_SUFFIX}"));
        let temp_path = parent.join(format!("{base}{RECORD_SUFFIX}{TEMP_SUFFIX}"));
        if target.exists() || record_path.exists() || temp_path.exists() {
            continue;
        }
        let mut file = match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let result = (|| -> Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp_path, &record_path)?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temp_path);
            return Err(error);
        }
        return Ok((
            target,
            record_path,
            RecoveryFile {
                path: PathBuf::from("sidecar"),
                size_bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            },
        ));
    }
    bail!("external tool recovery path could not be reserved")
}

fn read_record(path: &Path, delivery: &ExternalToolDelivery) -> Result<LoadedRecord> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > MAX_RECORD_BYTES {
        bail!("external tool recovery metadata is unsafe");
    }
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() {
        bail!("external tool recovery metadata changed while reading");
    }
    let value: RecoveryRecord =
        serde_json::from_slice(&bytes).context("external tool recovery metadata is invalid")?;
    validate_record(&value, delivery)?;
    Ok(LoadedRecord {
        value,
        record_file: RecoveryFile {
            path: PathBuf::from("sidecar"),
            size_bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        },
    })
}

fn validate_record(record: &RecoveryRecord, delivery: &ExternalToolDelivery) -> Result<()> {
    if record.schema_version != 1
        || record.id != delivery.id
        || record.reason.is_empty()
        || record.reason.chars().count() > MAX_REASON_CHARS
        || record.files.len() > delivery.files.len() + 1
    {
        bail!("external tool recovery metadata does not match this build");
    }
    crate::component_registry::component_version_root(&record.id, &record.version)?;
    let mut paths = HashSet::new();
    for file in &record.files {
        validate_relative_path(&file.path)?;
        if !paths.insert(file.path.clone())
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("external tool recovery file inventory is invalid");
        }
        let maximum = if file.path == Path::new(RECEIPT_NAME) {
            Some(MAX_RECEIPT_SNAPSHOT_BYTES)
        } else {
            delivery
                .files
                .iter()
                .find(|expected| Path::new(expected.path) == file.path)
                .map(|expected| expected.size_bytes)
        };
        if maximum.is_none_or(|maximum| file.size_bytes > maximum) {
            bail!("external tool recovery file is outside the compiled inventory");
        }
    }
    Ok(())
}

fn push_preserved(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if paths.len() < MAX_PRESERVED_PATHS && !paths.contains(&path) {
        paths.push(path);
    }
}

fn recovery_parent(id: &str) -> PathBuf {
    recovery_root().join(id)
}

#[cfg(not(test))]
fn recovery_root() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("component-recovery")
}

#[cfg(test)]
fn recovery_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "screen-goated-toolbox-component-recovery-tests-{}",
        std::process::id()
    ))
}

fn sidecar_target(path: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let name = path.file_name()?.to_str()?;
    let base = name.strip_suffix(RECORD_SUFFIX)?;
    if super::super::catalog::validate_identifier(base).is_err()
        || base.starts_with('.')
        || base.ends_with(['.', ' '])
        || base.contains("..")
        || recovery_io::is_windows_reserved_name(base)
    {
        return None;
    }
    let mut components = Path::new(base).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return None;
    }
    Some(path.parent()?.join(base))
}

fn bounded_reason(reason: &str) -> String {
    let value = reason.chars().take(MAX_REASON_CHARS).collect::<String>();
    if value.trim().is_empty() {
        "Installed component failed exact integrity validation.".to_string()
    } else {
        value
    }
}

fn preserved_outcome(path: PathBuf) -> RecoveryCleanupOutcome {
    RecoveryCleanupOutcome {
        path: path.clone(),
        removed_files: 0,
        preserved_paths: vec![path],
    }
}

#[cfg(test)]
pub(super) fn quarantine_with_rename_for_test(
    delivery: &ExternalToolDelivery,
    reason: &str,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<Option<PathBuf>> {
    quarantine_with(delivery, reason, rename)
}

#[cfg(test)]
pub(super) fn sidecar_target_for_test(path: &Path) -> Option<PathBuf> {
    sidecar_target(path)
}

#[cfg(test)]
pub(super) fn list_for_test(delivery: &ExternalToolDelivery) -> Result<Vec<ExternalToolRecovery>> {
    list_for(delivery.id, Some(delivery))
}

#[cfg(test)]
pub(super) fn clean_for_test(
    delivery: &ExternalToolDelivery,
    recovery: &ExternalToolRecovery,
) -> Result<RecoveryCleanupOutcome> {
    clean_for(delivery, recovery)
}

#[cfg(test)]
pub(super) fn purge_for_test(
    delivery: &ExternalToolDelivery,
    recovery: &ExternalToolRecovery,
) -> Result<RecoveryCleanupOutcome> {
    let record_path = recovery
        .record_path
        .as_deref()
        .ok_or_else(|| anyhow!("test recovery has no valid record"))?;
    let loaded = read_record(record_path, delivery)?;
    purge_loaded(&recovery.path, record_path, loaded)
}
