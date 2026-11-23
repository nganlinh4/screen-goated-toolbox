use std::collections::{HashMap, HashSet};
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

mod ownership;
#[cfg(test)]
mod proof_tests;

use super::{MAX_COMPONENT_FILES, RecorderDelivery, staging, version_root};
use crate::component_registry::receipt::{
    RECEIPT_NAME, is_reparse_point, resolve_owned_path, validate_relative_path,
};

const RECORD_SUFFIX: &str = ".recovery.json";
const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REASON_CHARS: usize = 2_048;
const MAX_RECOVERIES: usize = 128;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
static RECOVERY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static AUTHENTICATED: LazyLock<Mutex<HashMap<PathBuf, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
pub(crate) struct CleanupOutcome {
    pub(crate) path: PathBuf,
    pub(crate) preserved_paths: Vec<PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryRecord {
    schema_version: u32,
    id: String,
    version: String,
    directory_name: String,
    reason: String,
    files: Vec<RecoveryFile>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryFile {
    path: PathBuf,
    size_bytes: u64,
    sha256: String,
    cleanable: bool,
    #[serde(default)]
    declared: bool,
}

#[cfg(test)]
fn quarantine_invalid(delivery: &RecorderDelivery, reason: &str) -> Result<Option<PathBuf>> {
    let _exclusive = crate::component_registry::lease::reserve_exclusive_mutation(delivery.id)?;
    quarantine_invalid_reserved(delivery, reason, &_exclusive)
}

pub(super) fn quarantine_invalid_reserved(
    delivery: &RecorderDelivery,
    reason: &str,
    exclusive: &crate::component_registry::lease::ExclusiveComponentMutation,
) -> Result<Option<PathBuf>> {
    if !exclusive.matches(delivery.id) {
        bail!("recorder recovery reservation does not match its component");
    }
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
    let sequence = RECOVERY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| anyhow!("system clock is before the Unix epoch"))?
        .as_nanos();
    let directory_name = format!(
        "{}-{created}-{}-{sequence}",
        delivery.version,
        std::process::id()
    );
    let target = parent.join(&directory_name);
    let record_path = parent.join(format!("{directory_name}{RECORD_SUFFIX}"));
    if target.exists() || record_path.exists() {
        bail!("recorder recovery target already exists");
    }
    let record = snapshot(delivery, &source, &directory_name, reason)?;
    let body = serde_json::to_vec_pretty(&record)?;
    if body.len() as u64 > MAX_RECORD_BYTES {
        bail!("recorder recovery metadata is too large");
    }
    write_new_synced(&record_path, &body)?;
    if let Err(error) = crate::component_registry::recovery::no_replace_move(&source, &target) {
        let _ = remove_exact_record(&record_path, &body);
        return Err(error);
    }
    AUTHENTICATED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(record_path, format!("{:x}", Sha256::digest(&body)));
    crate::log_info!(
        "[Components] preserved invalid {} bytes at {}",
        delivery.id,
        target.display()
    );
    Ok(Some(target))
}

#[cfg(test)]
pub(crate) fn clean_all() -> Result<Vec<CleanupOutcome>> {
    clean_all_with(false)
}

pub(crate) fn purge_all_recorded() -> Result<Vec<CleanupOutcome>> {
    clean_all_with(true)
}

pub(crate) fn list() -> Result<Vec<(PathBuf, String)>> {
    let mut results = Vec::new();
    for id in [super::BUNDLE_ID, super::WEB_ID, super::WORKER_ID] {
        let parent = recovery_root().join(id);
        let metadata = match std::fs::symlink_metadata(&parent) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            results.push((
                parent,
                "Recorder recovery directory is unsafe; files are preserved.".into(),
            ));
            continue;
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&parent)? {
            if entries.len() >= MAX_RECOVERIES {
                bail!("recorder recovery inventory is too large");
            }
            entries.push(entry?.path());
        }
        let mut represented = HashSet::new();
        for sidecar in &entries {
            let Some(name) = sidecar
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(RECORD_SUFFIX))
            else {
                continue;
            };
            let target = parent.join(name);
            represented.insert(target.clone());
            match read_record(sidecar).and_then(|record| {
                validate_record(&record, id)?;
                if record.directory_name != name {
                    bail!("recorder recovery target does not match its record");
                }
                Ok(record)
            }) {
                Ok(record) => results.push((target, record.reason)),
                Err(_) => results.push((
                    sidecar.clone(),
                    "Recorder recovery metadata is invalid; files are preserved.".into(),
                )),
            }
        }
        for entry in entries {
            let is_sidecar = entry
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(RECORD_SUFFIX));
            if !is_sidecar && !represented.contains(&entry) {
                results.push((
                    entry,
                    "Recorder recovery metadata is missing; files are preserved.".into(),
                ));
            }
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

fn clean_all_with(delete_recorded: bool) -> Result<Vec<CleanupOutcome>> {
    let mut outcomes = Vec::new();
    for id in [super::BUNDLE_ID, super::WEB_ID, super::WORKER_ID] {
        let parent = recovery_root().join(id);
        let Ok(metadata) = std::fs::symlink_metadata(&parent) else {
            continue;
        };
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            outcomes.push(CleanupOutcome {
                path: parent.clone(),
                preserved_paths: vec![parent],
            });
            continue;
        }
        let mut records = Vec::new();
        let mut scanned = 0_usize;
        for entry in std::fs::read_dir(&parent)? {
            scanned += 1;
            if scanned > MAX_RECOVERIES {
                bail!("recorder recovery inventory is too large");
            }
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json")
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.ends_with(RECORD_SUFFIX))
            {
                records.push(path);
            }
        }
        for record_path in records {
            match clean_record(id, &parent, &record_path, delete_recorded) {
                Ok(outcome) => outcomes.push(outcome),
                Err(error) => {
                    crate::log_info!(
                        "[Components] could not clean recorder recovery {}: {error:#}",
                        record_path.display()
                    );
                    outcomes.push(CleanupOutcome {
                        path: record_path.clone(),
                        preserved_paths: vec![record_path],
                    });
                }
            }
        }
    }
    Ok(outcomes)
}

fn clean_record(
    id: &str,
    parent: &Path,
    record_path: &Path,
    delete_recorded: bool,
) -> Result<CleanupOutcome> {
    let record = read_record(record_path)?;
    validate_record(&record, id)?;
    let body = serde_json::to_vec_pretty(&record)?;
    let digest = format!("{:x}", Sha256::digest(&body));
    if !AUTHENTICATED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(record_path)
        .is_some_and(|expected| expected == &digest)
    {
        bail!("recorder recovery ownership cannot be verified; files were preserved");
    }
    let expected_record_name = format!("{}{RECORD_SUFFIX}", record.directory_name);
    if record_path.file_name().and_then(|name| name.to_str()) != Some(expected_record_name.as_str())
    {
        bail!("recorder recovery record does not match its target");
    }
    let target = parent.join(&record.directory_name);
    if target.parent() != Some(parent) {
        bail!("recorder recovery target escaped its directory");
    }
    let metadata = std::fs::symlink_metadata(&target)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("recorder recovery target is unsafe and was preserved");
    }
    let mut preserved = Vec::new();
    for file in &record.files {
        let path = match resolve_owned_path(&target, &file.path) {
            Ok(path) => path,
            Err(_) => {
                preserved.push(target.join(&file.path));
                continue;
            }
        };
        if !file.cleanable && !(delete_recorded && file.declared) {
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && file.declared => {
                    crate::component_registry::recovery::remove_empty_parents(
                        path.parent(),
                        &target,
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => preserved.push(path),
            }
            continue;
        }
        let expected = (!delete_recorded).then_some((file.size_bytes, file.sha256.as_str()));
        match crate::component_registry::recovery::delete_regular(&path, expected) {
            Ok(true) => {
                crate::component_registry::recovery::remove_empty_parents(path.parent(), &target)
            }
            Ok(false) | Err(_) => preserved.push(path),
        }
    }
    let _ = crate::component_registry::recovery::delete_empty_directory(&target);
    match std::fs::symlink_metadata(&target) {
        Ok(_) => {
            match crate::component_registry::owned_tree::regular_files(
                &target,
                "recorder component",
                MAX_COMPONENT_FILES + 1,
            ) {
                Ok(remaining) => {
                    for relative in remaining {
                        let path = target.join(relative);
                        if !preserved.contains(&path) {
                            preserved.push(path);
                        }
                    }
                }
                Err(_) => preserved.push(target.clone()),
            }
            if preserved.is_empty() {
                preserved.push(target.clone());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !remove_exact_record(record_path, &body) {
                preserved.push(record_path.to_path_buf());
            }
        }
        Err(_) => preserved.push(target.clone()),
    }
    preserved.sort();
    preserved.dedup();
    Ok(CleanupOutcome {
        path: target,
        preserved_paths: preserved,
    })
}

fn snapshot(
    delivery: &RecorderDelivery,
    source: &Path,
    directory_name: &str,
    reason: &str,
) -> Result<RecoveryRecord> {
    let paths = crate::component_registry::owned_tree::regular_files(
        source,
        "recorder component",
        MAX_COMPONENT_FILES + 1,
    )?;
    let mut files = Vec::with_capacity(paths.len());
    let mut total = 0_u64;
    let receipt_path = source.join(RECEIPT_NAME);
    let verified_receipt = snapshot_file(&receipt_path)
        .ok()
        .filter(|(_, hash)| ownership::valid_receipt(&receipt_path, delivery, hash))
        .map(|(_, hash)| hash);
    for path in paths {
        validate_relative_path(&path)?;
        let absolute = source.join(&path);
        let (size_bytes, sha256) = snapshot_file(&absolute)?;
        total = total
            .checked_add(size_bytes)
            .ok_or_else(|| anyhow!("recorder recovery is too large"))?;
        if total > MAX_TOTAL_BYTES {
            bail!("recorder recovery is too large");
        }
        let cleanable = if path == Path::new(RECEIPT_NAME) {
            verified_receipt.as_ref() == Some(&sha256)
        } else if let Some(expected) = delivery
            .files
            .iter()
            .find(|expected| Path::new(expected.path) == path)
        {
            size_bytes == expected.size_bytes && sha256.eq_ignore_ascii_case(expected.sha256)
        } else {
            false
        };
        files.push(RecoveryFile {
            declared: verified_receipt.is_some()
                && delivery
                    .files
                    .iter()
                    .any(|expected| Path::new(expected.path) == path),
            path,
            size_bytes,
            sha256,
            cleanable: verified_receipt.is_some() && cleanable,
        });
    }
    if !files
        .iter()
        .any(|file| file.path == Path::new(RECEIPT_NAME) && file.cleanable)
    {
        for file in &mut files {
            file.cleanable = false;
            file.declared = false;
        }
    }
    Ok(RecoveryRecord {
        schema_version: 1,
        id: delivery.id.to_string(),
        version: delivery.version.to_string(),
        directory_name: directory_name.to_string(),
        reason: reason.chars().take(MAX_REASON_CHARS).collect(),
        files,
    })
}

fn snapshot_file(path: &Path) -> Result<(u64, String)> {
    let before = std::fs::symlink_metadata(path)?;
    if !before.is_file() || is_reparse_point(&before) || before.len() > MAX_FILE_BYTES {
        bail!("recorder recovery contains an unsafe file");
    }
    let hash = crate::component_registry::recovery::hash_regular(path, before.len())?;
    Ok((before.len(), hash))
}

fn read_record(path: &Path) -> Result<RecoveryRecord> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > MAX_RECORD_BYTES {
        bail!("recorder recovery metadata is unsafe");
    }
    let mut body = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut body)?;
    serde_json::from_slice(&body).context("recorder recovery metadata is invalid")
}

fn validate_record(record: &RecoveryRecord, expected_id: &str) -> Result<()> {
    crate::component_registry::catalog::validate_identifier(&record.id)?;
    crate::component_registry::catalog::validate_identifier(&record.version)?;
    if record.schema_version != 1
        || record.id != expected_id
        || record.reason.chars().count() > MAX_REASON_CHARS
        || record.files.len() > MAX_COMPONENT_FILES + 1
    {
        bail!("recorder recovery metadata does not match its component");
    }
    let mut directory = Path::new(&record.directory_name).components();
    if !matches!(directory.next(), Some(Component::Normal(_))) || directory.next().is_some() {
        bail!("recorder recovery directory name is unsafe");
    }
    for file in &record.files {
        validate_relative_path(&file.path)?;
        if file.size_bytes > MAX_FILE_BYTES || file.sha256.len() != 64 {
            bail!("recorder recovery file metadata is invalid");
        }
    }
    Ok(())
}

fn write_new_synced(path: &Path, body: &[u8]) -> Result<()> {
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    output.write_all(body)?;
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn remove_exact_record(path: &Path, expected: &[u8]) -> bool {
    let hash = format!("{:x}", Sha256::digest(expected));
    crate::component_registry::recovery::delete_regular(path, Some((expected.len() as u64, &hash)))
        .unwrap_or(false)
}

#[cfg(not(test))]
fn recovery_root() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("component-recovery")
}

#[cfg(test)]
fn recovery_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "screen-goated-toolbox-recorder-recovery-tests-{}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    static FILES: [super::super::RecorderFile; 1] = [super::super::RecorderFile {
        path: "assets/app.js",
        size_bytes: 7,
        sha256: "7fdfda5f50a433ae127a784fc143105fb6d93fedec7601ddeb3d1d584f83de05",
    }];
    static DELIVERY: super::super::RecorderDelivery = super::super::RecorderDelivery {
        id: super::super::WEB_ID,
        version: "5.4.3",
        asset: "unused.zip",
        download_url: "https://example.invalid/unused.zip",
        size_bytes: 1,
        sha256: "00",
        unpacked_size_bytes: 7,
        files: &FILES,
    };

    #[test]
    #[ignore = "mutates the isolated process recorder recovery root"]
    fn tampered_install_is_quarantined_and_cleanup_preserves_user_bytes() {
        let source =
            crate::component_registry::ensure_version_root(DELIVERY.id, DELIVERY.version).unwrap();
        std::fs::create_dir(source.join("assets")).unwrap();
        std::fs::write(source.join("assets/app.js"), b"managed").unwrap();
        crate::component_registry::write_receipt(&source, &super::super::receipt(&DELIVERY))
            .unwrap();
        std::fs::write(source.join("assets/app.js"), b"tampered").unwrap();
        std::fs::write(source.join("user-note.txt"), b"user-owned").unwrap();

        let recovery = quarantine_invalid(&DELIVERY, "integrity mismatch")
            .unwrap()
            .unwrap();
        assert!(!source.exists());
        assert_eq!(
            std::fs::read(recovery.join("assets/app.js")).unwrap(),
            b"tampered"
        );
        assert_eq!(
            std::fs::read(recovery.join("user-note.txt")).unwrap(),
            b"user-owned"
        );

        let outcomes = clean_all().unwrap();
        let outcome = outcomes
            .iter()
            .find(|outcome| outcome.path == recovery)
            .unwrap();
        assert_eq!(outcome.preserved_paths.len(), 2);
        assert!(!recovery.join(RECEIPT_NAME).exists());
        std::fs::remove_file(recovery.join("assets/app.js")).unwrap();
        std::fs::remove_file(recovery.join("user-note.txt")).unwrap();
        assert!(
            clean_all()
                .unwrap()
                .iter()
                .any(|outcome| outcome.path == recovery)
        );
        assert!(!recovery.exists());
    }

    #[test]
    #[ignore = "mutates the isolated process recorder recovery root"]
    fn explicit_purge_removes_declared_modified_bytes_and_preserves_unknown_files() {
        let source =
            crate::component_registry::ensure_version_root(DELIVERY.id, DELIVERY.version).unwrap();
        std::fs::create_dir(source.join("assets")).unwrap();
        std::fs::write(source.join("assets/app.js"), b"changed").unwrap();
        std::fs::write(source.join("user-note.txt"), b"user-owned").unwrap();
        crate::component_registry::write_receipt(&source, &super::super::receipt(&DELIVERY))
            .unwrap();
        let recovery = quarantine_invalid(&DELIVERY, "integrity mismatch")
            .unwrap()
            .unwrap();

        assert!(recovery.exists());
        let outcomes = purge_all_recorded().unwrap();
        assert!(outcomes.iter().any(|outcome| outcome.path == recovery));
        assert!(!recovery.join("assets/app.js").exists());
        assert_eq!(
            std::fs::read(recovery.join("user-note.txt")).unwrap(),
            b"user-owned"
        );
        std::fs::remove_file(recovery.join("user-note.txt")).unwrap();
        purge_all_recorded().unwrap();
        assert!(!recovery.exists());
    }
}
