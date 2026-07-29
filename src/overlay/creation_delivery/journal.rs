use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::{CancelledDelivery, DeliveryStore, PublishedDelivery};

pub(super) fn journal_path() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("creation-deliveries.json")
}

pub(super) fn load_store(path: &Path) -> Result<DeliveryStore, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DeliveryStore {
                version: super::VERSION,
                entries: Vec::new(),
                cancellations: Vec::new(),
            });
        }
        Err(_) => return Err("Creation delivery state is unavailable.".to_string()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    if !metadata.is_file() || metadata.len() > super::MAX_FILE_BYTES {
        return Err("Creation delivery state is invalid.".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(super::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Creation delivery state is invalid.".to_string())?;
    if bytes.len() as u64 > super::MAX_FILE_BYTES {
        return Err("Creation delivery state is invalid.".to_string());
    }
    let store: DeliveryStore = serde_json::from_slice(&bytes)
        .map_err(|_| "Creation delivery state is invalid.".to_string())?;
    validate_store(&store)?;
    Ok(store)
}

fn validate_store(store: &DeliveryStore) -> Result<(), String> {
    if store.version != super::VERSION
        || store.entries.len() > super::MAX_ENTRIES
        || store.cancellations.len() > super::MAX_CANCELLATIONS
    {
        return Err("Creation delivery state is invalid.".to_string());
    }
    let mut dispatch_ids = HashSet::new();
    let mut job_ids = HashSet::new();
    let mut output_paths = HashSet::new();
    if store.entries.iter().any(|entry| {
        super::validate_input(&PublishedDelivery {
            product: match entry.product.as_str() {
                "3d" => "3d",
                "svg" => "svg",
                "image" => "image",
                _ => return true,
            },
            job_id: entry.job_id.clone(),
            dispatch_id: entry.dispatch_id.clone(),
            request_fingerprint: entry.request_fingerprint.clone(),
            source_path: entry.source_path.clone(),
            output_name: entry.output_name.clone(),
            staging_path: entry.staging_path.clone(),
            output_path: entry.output_path.clone(),
            metadata: entry.metadata.clone(),
        })
        .is_err()
            || super::publication::validate_paths(entry).is_err()
            || entry.artifact_sha256.len() != 64
            || !entry
                .artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !dispatch_ids.insert(entry.dispatch_id.clone())
            || !job_ids.insert((entry.product.clone(), entry.job_id.clone()))
            || super::publication::output_identity(Path::new(&entry.output_path))
                .map(|identity| !output_paths.insert(identity))
                .unwrap_or(true)
    }) {
        return Err("Creation delivery state is invalid.".to_string());
    }
    let mut cancellation_dispatches = HashSet::new();
    let mut cancellation_jobs = HashSet::new();
    if store.cancellations.iter().any(|cancellation| {
        super::validate_cancellation(&CancelledDelivery {
            product: match cancellation.product.as_str() {
                "3d" => "3d",
                "svg" => "svg",
                "image" => "image",
                _ => return true,
            },
            job_id: cancellation.job_id.clone(),
            dispatch_id: cancellation.dispatch_id.clone(),
            request_fingerprint: cancellation.request_fingerprint.clone(),
            output_name: cancellation.output_name.clone(),
        })
        .is_err()
            || cancellation.expires_at_ms <= cancellation.created_at_ms
            || cancellation.expires_at_ms
                > cancellation
                    .created_at_ms
                    .saturating_add(super::CANCELLATION_LIFETIME_MS)
            || !cancellation_dispatches.insert(cancellation.dispatch_id.clone())
            || !cancellation_jobs
                .insert((cancellation.product.clone(), cancellation.job_id.clone()))
            || store.entries.iter().any(|entry| {
                entry.dispatch_id == cancellation.dispatch_id
                    && (!entry.stage.is_pre_publication()
                        || entry.product != cancellation.product
                        || entry.job_id != cancellation.job_id
                        || entry.request_fingerprint != cancellation.request_fingerprint
                        || entry.output_name != cancellation.output_name)
            })
    }) {
        return Err("Creation delivery state is invalid.".to_string());
    }
    Ok(())
}

pub(super) fn save_store(path: &Path, store: &DeliveryStore) -> Result<(), String> {
    let bytes = serde_json::to_vec(store)
        .map_err(|_| "Creation delivery state could not be saved.".to_string())?;
    let cancellation_reserve = crate::overlay::creation_intent_journal::load_all()?
        .len()
        .saturating_add(1) as u64
        * super::MAX_SERIALIZED_CANCELLATION_BYTES;
    if (bytes.len() as u64).saturating_add(cancellation_reserve) > super::MAX_FILE_BYTES {
        return Err("Creation delivery state is too large.".to_string());
    }
    crate::atomic_json::write_json_atomic(path, store)
        .map_err(|_| "Creation delivery state could not be saved.".to_string())
}
