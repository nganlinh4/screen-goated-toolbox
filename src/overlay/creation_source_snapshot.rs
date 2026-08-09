//! Durable, immutable source ownership for accepted creation dispatches.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use super::creation_source::{InspectedImage, SourceDescriptor};

mod presentation;
mod storage;
mod sweep;
pub(crate) use presentation::presentation_path;
#[cfg(test)]
pub(crate) use presentation::presentation_paths;
use storage::*;
pub(crate) use sweep::{sweep, sweep_pressure};

const VERSION: u32 = 1;
const ROOT_NAME: &str = "creation-source-snapshots";
const PREVIEW_ROOT_NAME: &str = "creation-source-previews";
const MANIFEST_NAME: &str = ".sgt-source.json";
const MAX_MANIFEST_BYTES: u64 = 128 * 1024;
const MAX_SOURCES: usize = 20;
const MAX_OWNER_IDS: usize = 8;
const ORPHAN_GRACE_MS: u64 = 60 * 60 * 1_000;

static SNAPSHOT_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotEntry {
    original_path: String,
    descriptor: SourceDescriptor,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContinuationOwner {
    id: String,
    expires_at_ms: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotManifest {
    version: u32,
    product: String,
    snapshot_id: String,
    created_at_ms: u64,
    intent_owners: Vec<String>,
    continuation: Option<ContinuationOwner>,
    entries: Vec<SnapshotEntry>,
    #[serde(default)]
    presentation_paths: Vec<String>,
}

pub(crate) struct SnapshotAssignment {
    snapshot_id: String,
    descriptors: Vec<SourceDescriptor>,
    armed: bool,
}

impl SnapshotAssignment {
    pub(crate) fn descriptors(&self) -> &[SourceDescriptor] {
        &self.descriptors
    }

    pub(crate) fn paths(&self) -> Vec<String> {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.path.clone())
            .collect()
    }

    pub(crate) fn persist(mut self) {
        self.armed = false;
    }
}

impl Drop for SnapshotAssignment {
    fn drop(&mut self) {
        if self.armed {
            let _ = cleanup_snapshot(&self.snapshot_id);
        }
    }
}

pub(crate) fn prepare(
    product: &str,
    dispatch_id: &str,
    sources: &[InspectedImage],
) -> Result<SnapshotAssignment, String> {
    if !matches!(product, "3d" | "svg" | "image")
        || !valid_id(dispatch_id)
        || sources.is_empty()
        || sources.len() > MAX_SOURCES
    {
        return Err("Creation source assignment is invalid.".to_string());
    }
    let root = snapshot_root()?;
    let directory = root.join(dispatch_id);
    std::fs::create_dir(&directory)
        .map_err(|_| "Creation source could not be prepared.".to_string())?;
    let directory = validate_snapshot_directory(&root, dispatch_id)?;
    let result = (|| {
        let mut entries = Vec::with_capacity(sources.len());
        let mut total_bytes = 0_u64;
        for (index, source) in sources.iter().enumerate() {
            total_bytes = total_bytes
                .checked_add(source.size_bytes)
                .ok_or_else(|| "Creation source size is invalid.".to_string())?;
            let source_limit = if product == "image" {
                super::generation_history::IMAGE_REFERENCE_RESERVATION_BYTES
            } else {
                super::creation_source::MAX_SOURCE_BYTES
            };
            if total_bytes > source_limit {
                return Err("Creation sources exceed their storage limit.".to_string());
            }
            let target = directory.join(format!(
                "source-{index:02}.{}",
                source.canonical_extension()
            ));
            copy_locked(source, &target)?;
            let inspected = super::creation_source::inspect_image(&target)?;
            if inspected.size_bytes != source.size_bytes || inspected.sha256 != source.sha256 {
                return Err("Creation source changed before it was accepted.".to_string());
            }
            entries.push(SnapshotEntry {
                original_path: source.path.to_string_lossy().to_string(),
                descriptor: inspected.descriptor(),
            });
        }
        let manifest = SnapshotManifest {
            version: VERSION,
            product: product.to_string(),
            snapshot_id: dispatch_id.to_string(),
            created_at_ms: now_ms(),
            intent_owners: vec![dispatch_id.to_string()],
            continuation: None,
            entries,
            presentation_paths: Vec::new(),
        };
        validate_manifest(&root, &manifest)?;
        write_manifest(&directory, &manifest)?;
        Ok(SnapshotAssignment {
            snapshot_id: dispatch_id.to_string(),
            descriptors: manifest
                .entries
                .iter()
                .map(|entry| entry.descriptor.clone())
                .collect(),
            armed: true,
        })
    })();
    if result.is_err() {
        let _ = cleanup_uncommitted_directory(&directory);
    }
    result
}

pub(crate) fn validate_sources(descriptors: &[SourceDescriptor]) -> Result<(), String> {
    if descriptors.is_empty() {
        return Ok(());
    }
    let root = snapshot_root()?;
    let snapshot_id = snapshot_id_from_descriptors(&root, descriptors)?;
    let directory = validate_snapshot_directory(&root, &snapshot_id)?;
    let manifest = read_manifest(&directory)?;
    validate_manifest(&root, &manifest)?;
    if manifest
        .entries
        .iter()
        .map(|entry| &entry.descriptor)
        .ne(descriptors)
    {
        return Err("Saved creation sources are invalid.".to_string());
    }
    for descriptor in descriptors {
        super::creation_source::revalidate_source(descriptor)?;
    }
    Ok(())
}

pub(crate) fn original_paths(descriptors: &[SourceDescriptor]) -> Result<Vec<String>, String> {
    if descriptors.is_empty() {
        return Ok(Vec::new());
    }
    let root = snapshot_root()?;
    let snapshot_id = snapshot_id_from_descriptors(&root, descriptors)?;
    let directory = validate_snapshot_directory(&root, &snapshot_id)?;
    let manifest = read_manifest(&directory)?;
    validate_manifest(&root, &manifest)?;
    if manifest
        .entries
        .iter()
        .map(|entry| &entry.descriptor)
        .ne(descriptors)
    {
        return Err("Saved creation sources are invalid.".to_string());
    }
    Ok(manifest
        .entries
        .into_iter()
        .map(|entry| entry.original_path)
        .collect())
}

pub(crate) fn claim_intent(
    descriptors: &[SourceDescriptor],
    dispatch_id: &str,
) -> Result<(), String> {
    if !valid_id(dispatch_id) {
        return Err("Creation source ownership is invalid.".to_string());
    }
    mutate_manifest(descriptors, |manifest| {
        if !manifest
            .intent_owners
            .iter()
            .any(|owner| owner == dispatch_id)
        {
            if manifest.intent_owners.len() >= MAX_OWNER_IDS {
                return Err("Creation source ownership is full.".to_string());
            }
            manifest.intent_owners.push(dispatch_id.to_string());
        }
        Ok(())
    })
}

pub(crate) fn retain_continuation(
    descriptors: &[SourceDescriptor],
    dispatch_id: &str,
    continuation_id: &str,
    expires_at_ms: u64,
) -> Result<(), String> {
    if !valid_id(dispatch_id) || !valid_id(continuation_id) || expires_at_ms <= now_ms() {
        return Err("Creation continuation is invalid.".to_string());
    }
    mutate_manifest(descriptors, |manifest| {
        if !manifest
            .intent_owners
            .iter()
            .any(|owner| owner == dispatch_id)
        {
            return Err("Creation source ownership is missing.".to_string());
        }
        match &manifest.continuation {
            Some(owner) if owner.id != continuation_id || owner.expires_at_ms != expires_at_ms => {
                Err("Creation continuation conflicts with saved state.".to_string())
            }
            Some(_) => Ok(()),
            None => {
                manifest.continuation = Some(ContinuationOwner {
                    id: continuation_id.to_string(),
                    expires_at_ms,
                });
                Ok(())
            }
        }
    })
}

pub(crate) fn release_continuation(
    descriptors: &[SourceDescriptor],
    continuation_id: &str,
) -> Result<(), String> {
    mutate_manifest(descriptors, |manifest| {
        if manifest
            .continuation
            .as_ref()
            .is_some_and(|owner| owner.id == continuation_id)
        {
            manifest.continuation = None;
        }
        Ok(())
    })?;
    cleanup_if_unowned(descriptors)
}

pub(super) fn record_presentations(
    descriptors: &[SourceDescriptor],
    paths: &[String],
) -> Result<(), String> {
    if paths.len() > descriptors.len() {
        return Err("Creation preview assignment is invalid.".to_string());
    }
    mutate_manifest(descriptors, |manifest| {
        if !manifest
            .presentation_paths
            .iter()
            .zip(paths)
            .all(|(saved, current)| saved == current)
        {
            return Err("Creation preview conflicts with saved state.".to_string());
        }
        if paths.len() > manifest.presentation_paths.len() {
            manifest.presentation_paths = paths.to_vec();
        }
        Ok(())
    })
}

pub(crate) fn remaining_presentation_reservation(
    descriptors: &[SourceDescriptor],
) -> Result<u64, String> {
    if descriptors.is_empty() {
        return Ok(0);
    }
    let root = snapshot_root()?;
    let snapshot_id = snapshot_id_from_descriptors(&root, descriptors)?;
    let directory = validate_snapshot_directory(&root, &snapshot_id)?;
    let manifest = read_manifest(&directory)?;
    validate_manifest(&root, &manifest)?;
    let remaining = descriptors
        .len()
        .saturating_sub(manifest.presentation_paths.len());
    super::generation_history::SOURCE_PRESENTATION_RESERVATION_BYTES
        .checked_mul(remaining as u64)
        .ok_or_else(|| "Saved creation preview reservation is invalid.".to_string())
}

pub(crate) fn release_for_cleared_intent(intent: &super::creation_intent_journal::Intent) {
    let descriptors = intent
        .arguments
        .get("sourceDescriptors")
        .and_then(|value| serde_json::from_value::<Vec<SourceDescriptor>>(value.clone()).ok())
        .unwrap_or_default();
    if descriptors.is_empty() {
        return;
    }
    let _ = release_intent(&descriptors, &intent.dispatch_id);
}

fn release_intent(descriptors: &[SourceDescriptor], dispatch_id: &str) -> Result<(), String> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| "Creation source ownership is unavailable.".to_string())?;
    let root = snapshot_root()?;
    let snapshot_id = snapshot_id_from_descriptors(&root, descriptors)?;
    let directory = validate_snapshot_directory(&root, &snapshot_id)?;
    let mut manifest = read_manifest(&directory)?;
    validate_manifest(&root, &manifest)?;
    manifest.intent_owners.retain(|owner| owner != dispatch_id);
    if manifest
        .continuation
        .as_ref()
        .is_some_and(|owner| owner.expires_at_ms <= now_ms())
    {
        manifest.continuation = None;
    }
    if manifest.intent_owners.is_empty() && manifest.continuation.is_none() {
        cleanup_manifest_directory(&directory, &manifest)
    } else {
        write_manifest(&directory, &manifest)
    }
}

fn cleanup_if_unowned(descriptors: &[SourceDescriptor]) -> Result<(), String> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| "Creation source ownership is unavailable.".to_string())?;
    let root = snapshot_root()?;
    let snapshot_id = snapshot_id_from_descriptors(&root, descriptors)?;
    let directory = validate_snapshot_directory(&root, &snapshot_id)?;
    let manifest = read_manifest(&directory)?;
    validate_manifest(&root, &manifest)?;
    if manifest.intent_owners.is_empty() && manifest.continuation.is_none() {
        cleanup_manifest_directory(&directory, &manifest)
    } else {
        Ok(())
    }
}

fn mutate_manifest(
    descriptors: &[SourceDescriptor],
    mutate: impl FnOnce(&mut SnapshotManifest) -> Result<(), String>,
) -> Result<(), String> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| "Creation source ownership is unavailable.".to_string())?;
    let root = snapshot_root()?;
    let snapshot_id = snapshot_id_from_descriptors(&root, descriptors)?;
    let directory = validate_snapshot_directory(&root, &snapshot_id)?;
    let mut manifest = read_manifest(&directory)?;
    validate_manifest(&root, &manifest)?;
    if manifest
        .entries
        .iter()
        .map(|entry| &entry.descriptor)
        .ne(descriptors)
    {
        return Err("Saved creation sources are invalid.".to_string());
    }
    mutate(&mut manifest)?;
    write_manifest(&directory, &manifest)
}

fn snapshot_id_from_descriptors(
    root: &Path,
    descriptors: &[SourceDescriptor],
) -> Result<String, String> {
    let first = descriptors
        .first()
        .ok_or_else(|| "Creation source assignment is missing.".to_string())?;
    let parent = Path::new(&first.path)
        .parent()
        .ok_or_else(|| "Creation source assignment is invalid.".to_string())?;
    if parent.parent() != Some(root)
        || descriptors
            .iter()
            .any(|descriptor| Path::new(&descriptor.path).parent() != Some(parent))
    {
        return Err("Creation source assignment is invalid.".to_string());
    }
    let id = parent
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| valid_id(value))
        .ok_or_else(|| "Creation source assignment is invalid.".to_string())?;
    Ok(id.to_string())
}

fn validate_manifest(root: &Path, manifest: &SnapshotManifest) -> Result<(), String> {
    if manifest.version != VERSION
        || !matches!(manifest.product.as_str(), "3d" | "svg" | "image")
        || !valid_id(&manifest.snapshot_id)
        || manifest.created_at_ms == 0
        || manifest.intent_owners.len() > MAX_OWNER_IDS
        || manifest.intent_owners.iter().any(|owner| !valid_id(owner))
        || manifest.continuation.as_ref().is_some_and(|owner| {
            !valid_id(&owner.id) || owner.expires_at_ms <= manifest.created_at_ms
        })
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_SOURCES
        || manifest.presentation_paths.len() > manifest.entries.len()
    {
        return Err("Saved creation source state is invalid.".to_string());
    }
    let preview_parent = root
        .parent()
        .ok_or_else(|| "Saved creation source state is invalid.".to_string())?
        .join(PREVIEW_ROOT_NAME);
    if manifest.presentation_paths.iter().any(|path| {
        let path = Path::new(path);
        path.parent() != Some(preview_parent.as_path())
            || path
                .file_name()
                .and_then(|value| value.to_str())
                .is_none_or(|name| {
                    name.len() != 68
                        || !name.ends_with(".png")
                        || !name[..64].bytes().all(|byte| byte.is_ascii_hexdigit())
                })
    }) {
        return Err("Saved creation source state is invalid.".to_string());
    }
    let directory = root.join(&manifest.snapshot_id);
    let mut paths = HashSet::new();
    for (index, entry) in manifest.entries.iter().enumerate() {
        let path = Path::new(&entry.descriptor.path);
        let expected_prefix = format!("source-{index:02}.");
        if entry.original_path.is_empty()
            || entry.original_path.len() > 8 * 1024
            || !Path::new(&entry.original_path).is_absolute()
            || path.parent() != Some(directory.as_path())
            || !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(&expected_prefix))
            || entry.descriptor.size_bytes == 0
            || entry.descriptor.size_bytes > super::creation_source::MAX_SOURCE_BYTES
            || entry.descriptor.sha256.len() != 64
            || !entry
                .descriptor
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !paths.insert(entry.descriptor.path.to_ascii_lowercase())
        {
            return Err("Saved creation source state is invalid.".to_string());
        }
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
