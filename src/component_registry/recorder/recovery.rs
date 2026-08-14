use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{MAX_COMPONENT_FILES, RecorderDelivery, owned_file, staging, version_root};
use crate::component_registry::receipt::{
    RECEIPT_NAME, file_matches, is_reparse_point, validate_relative_path,
};

const RECORD_SUFFIX: &str = ".recovery.json";
const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REASON_CHARS: usize = 2_048;
const MAX_RECOVERIES: usize = 128;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
static RECOVERY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
}

pub(super) fn quarantine_invalid(
    delivery: &RecorderDelivery,
    reason: &str,
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
    if let Err(error) = std::fs::rename(&source, &target) {
        remove_exact_record(&record_path, &body);
        return Err(error.into());
    }
    crate::log_info!(
        "[Components] preserved invalid {} bytes at {}",
        delivery.id,
        target.display()
    );
    Ok(Some(target))
}

pub(crate) fn clean_all() -> Result<Vec<CleanupOutcome>> {
    let mut outcomes = Vec::new();
    for id in [super::WEB_ID, super::WORKER_ID] {
        let parent = recovery_root().join(id);
        let Ok(metadata) = std::fs::symlink_metadata(&parent) else {
            continue;
        };
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            bail!("recorder recovery directory is unsafe");
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
            outcomes.push(clean_record(id, &parent, &record_path)?);
        }
    }
    Ok(outcomes)
}

fn clean_record(id: &str, parent: &Path, record_path: &Path) -> Result<CleanupOutcome> {
    let record = read_record(record_path)?;
    validate_record(&record, id)?;
    let expected_record_name = format!("{}{RECORD_SUFFIX}", record.directory_name);
    if record_path.file_name().and_then(|name| name.to_str()) != Some(expected_record_name.as_str())
    {
        bail!("recorder recovery record does not match its target");
    }
    let target = parent.join(&record.directory_name);
    if target.parent() != Some(parent) {
        bail!("recorder recovery target escaped its directory");
    }
    let mut removable = Vec::new();
    let mut preserved = Vec::new();
    for file in &record.files {
        let path = target.join(&file.path);
        if !path.exists() || (file.cleanable && snapshot_matches(&path, file)?) {
            removable.push(file.path.clone());
        } else {
            preserved.push(path);
        }
    }
    staging::cleanup_owned(&target, &removable)?;
    if target.exists() {
        let mut remaining = Vec::new();
        staging::collect_regular_files(&target, &target, &mut remaining, MAX_COMPONENT_FILES + 1)?;
        for relative in remaining {
            let path = target.join(relative);
            if !preserved.contains(&path) {
                preserved.push(path);
            }
        }
    } else {
        remove_record(record_path)?;
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
    let mut paths = Vec::new();
    staging::collect_regular_files(source, source, &mut paths, MAX_COMPONENT_FILES + 1)?;
    let mut files = Vec::with_capacity(paths.len());
    let mut total = 0_u64;
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
            true
        } else if let Some(expected) = delivery
            .files
            .iter()
            .find(|expected| Path::new(expected.path) == path)
        {
            file_matches(&absolute, &owned_file(expected))?
        } else {
            false
        };
        files.push(RecoveryFile {
            path,
            size_bytes,
            sha256,
            cleanable,
        });
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
    let mut input = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let copied = std::io::copy(&mut input, &mut digest)?;
    let after = input.metadata()?;
    if copied != before.len()
        || after.len() != before.len()
        || after.modified().ok() != before.modified().ok()
    {
        bail!("recorder recovery source changed while hashing");
    }
    Ok((copied, format!("{:x}", digest.finalize())))
}

fn snapshot_matches(path: &Path, expected: &RecoveryFile) -> Result<bool> {
    let owned = crate::component_registry::OwnedComponentFile {
        path: expected.path.clone(),
        size_bytes: expected.size_bytes,
        sha256: expected.sha256.clone(),
    };
    file_matches(path, &owned)
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

fn remove_exact_record(path: &Path, expected: &[u8]) {
    if std::fs::read(path).is_ok_and(|body| body == expected) {
        let _ = std::fs::remove_file(path);
    }
}

fn remove_record(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("recorder recovery record is unsafe");
    }
    std::fs::remove_file(path)?;
    Ok(())
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
}
