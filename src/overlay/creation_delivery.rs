//! Durable completion handoff for creation results.
//!
//! One user dispatch owns one published artifact and one history commit. A
//! restart replays only the unfinished handoff; it never equates equal content
//! with the same user action.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod cancellation;
mod journal;
pub(crate) mod publication;
mod retry;

pub(crate) use cancellation::{
    cancel_dispatch, cancel_product_intents, ensure_cancellation_capacity,
};
use journal::{journal_path, load_store, save_store};
#[cfg(test)]
use retry::retry_start_delay_ms;
pub(crate) use retry::schedule_reconciliation;

const VERSION: u32 = 1;
const MAX_ENTRIES: usize = 192;
const MAX_CANCELLATIONS: usize = 8_000;
const MAX_SERIALIZED_CANCELLATION_BYTES: u64 = 2_048;
const CANCELLATION_LIFETIME_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAX_FILE_BYTES: u64 = 24 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 512 * 1024;
const MAX_PATH_BYTES: usize = 8 * 1024;
const MAX_ID_BYTES: usize = 512;

static DELIVERY_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone)]
pub struct PublishedDelivery {
    pub product: &'static str,
    pub job_id: String,
    pub dispatch_id: String,
    pub request_fingerprint: String,
    pub source_path: String,
    pub output_name: String,
    pub staging_path: String,
    pub output_path: String,
    pub metadata: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum DeliveryStage {
    Validated,
    PublicationPrepared,
    Published,
    HistoryCommitted,
}

impl DeliveryStage {
    fn is_pre_publication(self) -> bool {
        matches!(self, Self::Validated | Self::PublicationPrepared)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryRecord {
    product: String,
    job_id: String,
    dispatch_id: String,
    request_fingerprint: String,
    source_path: String,
    output_name: String,
    staging_path: String,
    output_path: String,
    publication_path: String,
    publication_claim: String,
    #[serde(default)]
    publication_file_identity: Option<String>,
    metadata: Value,
    artifact_size_bytes: u64,
    artifact_sha256: String,
    stage: DeliveryStage,
}

#[derive(Clone)]
pub(crate) struct CancelledDelivery {
    pub product: &'static str,
    pub job_id: String,
    pub dispatch_id: String,
    pub request_fingerprint: String,
    pub output_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancellationRecord {
    product: String,
    job_id: String,
    dispatch_id: String,
    request_fingerprint: String,
    output_name: String,
    created_at_ms: u64,
    expires_at_ms: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct DeliveryStore {
    version: u32,
    entries: Vec<DeliveryRecord>,
    #[serde(default)]
    cancellations: Vec<CancellationRecord>,
}

pub fn commit(delivery: PublishedDelivery) -> Result<(), String> {
    validate_input(&delivery)?;
    crate::overlay::creation_close::ensure_accepting(delivery.product)?;
    verify_active_intent(
        delivery.product,
        &delivery.job_id,
        &delivery.dispatch_id,
        &delivery.request_fingerprint,
        &delivery.output_name,
        &delivery.staging_path,
        &delivery.output_path,
    )?;
    let product = delivery.product;
    let artifact =
        crate::overlay::generation_history::inspect_delivery_artifact(&delivery.staging_path)?;
    let result = (|| {
        let _guard = DELIVERY_LOCK
            .lock()
            .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
        crate::overlay::creation_close::ensure_accepting(delivery.product)?;
        let path = journal_path();
        let mut store = load_store(&path)?;
        if store
            .cancellations
            .iter()
            .any(|cancelled| cancelled.dispatch_id == delivery.dispatch_id)
        {
            let _ = cleanup_delivery_staging(
                &delivery.dispatch_id,
                &delivery.output_name,
                &delivery.staging_path,
            );
            return Err("Creation delivery was cancelled.".to_string());
        }
        let existing = store
            .entries
            .iter()
            .position(|entry| entry.dispatch_id == delivery.dispatch_id);
        let dispatch_id = delivery.dispatch_id.clone();
        if let Some(index) = existing {
            let saved = &store.entries[index];
            validate_saved_delivery(saved, &delivery, &artifact)?;
        } else {
            // The first verification happens before artifact inspection. Repeat it
            // under the delivery lock so a cancellation that cleared the intent
            // while inspection was running cannot be recreated as a new entry.
            verify_active_intent(
                delivery.product,
                &delivery.job_id,
                &delivery.dispatch_id,
                &delivery.request_fingerprint,
                &delivery.output_name,
                &delivery.staging_path,
                &delivery.output_path,
            )?;
            let output_identity = publication::output_identity(Path::new(&delivery.output_path))?;
            if store.entries.len() >= MAX_ENTRIES
                || store.entries.iter().any(|entry| {
                    entry.product == delivery.product && entry.job_id == delivery.job_id
                })
                || store.entries.iter().any(|entry| {
                    publication::output_identity(Path::new(&entry.output_path))
                        .is_ok_and(|identity| identity == output_identity)
                })
            {
                return Err("Creation delivery state is full or conflicting.".to_string());
            }
            let publication_path =
                publication::reserve_path(Path::new(&delivery.output_path), &delivery.dispatch_id)?;
            let publication_claim = publication::new_claim()?;
            store.entries.push(DeliveryRecord {
                product: delivery.product.to_string(),
                job_id: delivery.job_id,
                dispatch_id: delivery.dispatch_id,
                request_fingerprint: delivery.request_fingerprint,
                source_path: delivery.source_path,
                output_name: delivery.output_name,
                staging_path: delivery.staging_path,
                output_path: delivery.output_path,
                publication_path: publication_path.to_string_lossy().to_string(),
                publication_claim,
                publication_file_identity: None,
                metadata: delivery.metadata,
                artifact_size_bytes: artifact.size_bytes,
                artifact_sha256: artifact.sha256,
                stage: DeliveryStage::Validated,
            });
            save_store(&path, &store)?;
        }
        advance_delivery(
            &path,
            &mut store,
            &dispatch_id,
            |record, artifact, protected_paths| {
                crate::overlay::generation_history::record_delivery(
                    &record.product,
                    &record.dispatch_id,
                    &record.source_path,
                    &record.output_path,
                    record.metadata.clone(),
                    artifact,
                    protected_paths,
                )
                .map(|_| ())
            },
            |record| {
                crate::overlay::creation_intent_journal::clear_required(
                    &record.product,
                    &record.job_id,
                )
            },
            save_store,
        )
    })();
    if result.is_err() {
        schedule_reconciliation(product);
    }
    result
}

pub fn reconcile_product(product: &str) -> Result<HashSet<String>, String> {
    validate_product(product)?;
    let _guard = DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    let path = journal_path();
    let mut store = load_store(&path)?;
    cancellation::finalize_cancellations(&path, &mut store, product)?;
    if crate::overlay::creation_close::is_closing(product) {
        return Ok(cancellation::pending_job_ids(&store, product));
    }
    let dispatches = store
        .entries
        .iter()
        .filter(|entry| entry.product == product)
        .map(|entry| entry.dispatch_id.clone())
        .collect::<Vec<_>>();
    for dispatch_id in dispatches {
        let result = cancellation::advance_saved_delivery(&path, &mut store, &dispatch_id);
        if let Err(error) = result {
            crate::log_info!("[Creation delivery] Completion is still pending: {error}");
        }
    }
    Ok(cancellation::pending_job_ids(&store, product))
}

pub(crate) fn pending_output_paths() -> Result<HashSet<String>, String> {
    let _guard = DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    Ok(load_store(&journal_path())?
        .entries
        .into_iter()
        .map(|entry| entry.output_path)
        .collect())
}

pub(crate) fn pending_source_paths() -> Result<HashSet<String>, String> {
    let _guard = DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    Ok(load_store(&journal_path())?
        .entries
        .into_iter()
        .filter_map(|entry| (!entry.source_path.is_empty()).then_some(entry.source_path))
        .collect())
}

#[derive(Clone, Debug)]
pub(crate) struct PendingStorageReservation {
    pub dispatch_id: String,
    pub output_path: String,
    pub additional_output_bytes: u64,
}

pub(crate) fn pending_storage_reservations() -> Result<Vec<PendingStorageReservation>, String> {
    let _guard = DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    Ok(load_store(&journal_path())?
        .entries
        .into_iter()
        .map(|entry| PendingStorageReservation {
            dispatch_id: entry.dispatch_id,
            output_path: entry.output_path,
            additional_output_bytes: if entry.stage == DeliveryStage::Validated {
                entry.artifact_size_bytes
            } else {
                0
            },
        })
        .collect())
}

pub(crate) fn protected_dispatch_ids() -> Result<HashSet<String>, String> {
    let _guard = DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    let store = load_store(&journal_path())?;
    Ok(store
        .entries
        .into_iter()
        .map(|entry| entry.dispatch_id)
        .chain(
            store
                .cancellations
                .into_iter()
                .map(|cancellation| cancellation.dispatch_id),
        )
        .collect())
}

fn advance_delivery(
    path: &Path,
    store: &mut DeliveryStore,
    dispatch_id: &str,
    mut record_history: impl FnMut(
        &DeliveryRecord,
        &crate::overlay::generation_history::DeliveryArtifactIdentity,
        &HashSet<String>,
    ) -> Result<(), String>,
    mut clear_intent: impl FnMut(&DeliveryRecord) -> Result<(), String>,
    mut save: impl FnMut(&Path, &DeliveryStore) -> Result<(), String>,
) -> Result<(), String> {
    let index = store
        .entries
        .iter()
        .position(|entry| entry.dispatch_id == dispatch_id)
        .ok_or_else(|| "Creation delivery state is missing.".to_string())?;
    if store
        .cancellations
        .iter()
        .any(|cancelled| cancelled.dispatch_id == dispatch_id)
    {
        let record = store.entries[index].clone();
        publication::cancel_pre_publication(&record)?;
        clear_intent(&record)?;
        cleanup_record_staging(&record)?;
        store.entries.remove(index);
        cancellation::retire(store, dispatch_id);
        save(path, store)?;
        return Err("Creation delivery was cancelled.".to_string());
    }
    if store.entries[index].stage != DeliveryStage::HistoryCommitted {
        let record = &store.entries[index];
        verify_active_intent(
            &record.product,
            &record.job_id,
            &record.dispatch_id,
            &record.request_fingerprint,
            &record.output_name,
            &record.staging_path,
            &record.output_path,
        )?;
    }
    if store.entries[index].stage == DeliveryStage::Validated {
        if store.entries[index].publication_file_identity.is_some() {
            match std::fs::symlink_metadata(&store.entries[index].publication_path) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    store.entries[index].publication_file_identity = None;
                    save(path, store)?;
                }
                Err(_) => {
                    return Err("Creation publication receipt is unavailable.".to_string());
                }
            }
        }
        if store.entries[index].publication_file_identity.is_none() {
            let reservation = publication::create_receipt(&store.entries[index])?;
            store.entries[index].publication_file_identity =
                Some(reservation.identity().to_string());
            save(path, store)?;
            reservation.commit()?;
        }
        let record = store.entries[index].clone();
        publication::prepare(&record)?;
        store.entries[index].stage = DeliveryStage::PublicationPrepared;
        save(path, store)?;
    }
    if store.entries[index].stage == DeliveryStage::PublicationPrepared {
        let record = store.entries[index].clone();
        let published = publication::publish_prepared(&record)?;
        store.entries[index].stage = DeliveryStage::Published;
        save(path, store)?;
        drop(published);
    }
    if store.entries[index].stage == DeliveryStage::Published {
        let record = store.entries[index].clone();
        let published = publication::verify_published(&record)?;
        let protected_paths = store
            .entries
            .iter()
            .map(|entry| entry.output_path.clone())
            .collect();
        record_history(&record, published.artifact(), &protected_paths)?;
        store.entries[index].stage = DeliveryStage::HistoryCommitted;
        save(path, store)?;
        drop(published);
    }
    let record = store.entries[index].clone();
    clear_intent(&record)?;
    cleanup_record_staging(&record)?;
    publication::cleanup_receipt(&record)?;
    store.entries.remove(index);
    save(path, store)
}

fn validate_input(delivery: &PublishedDelivery) -> Result<(), String> {
    validate_product(delivery.product)?;
    if !valid_id(&delivery.job_id)
        || !valid_id(&delivery.dispatch_id)
        || delivery.source_path.len() > MAX_PATH_BYTES
        || delivery.staging_path.len() > MAX_PATH_BYTES
        || delivery.output_path.len() > MAX_PATH_BYTES
        || !Path::new(&delivery.staging_path).is_absolute()
        || !Path::new(&delivery.output_path).is_absolute()
        || !valid_fingerprint(&delivery.request_fingerprint)
        || serde_json::to_vec(&delivery.metadata)
            .map(|bytes| bytes.len() > MAX_METADATA_BYTES)
            .unwrap_or(true)
    {
        return Err("Creation delivery state is invalid.".to_string());
    }
    let output_name = Path::new(&delivery.output_path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Creation delivery state is invalid.".to_string())?;
    if output_name != delivery.output_name
        || !valid_product_output_name(delivery.product, output_name)
    {
        return Err("Creation delivery output assignment is invalid.".to_string());
    }
    publication::output_identity(Path::new(&delivery.output_path))?;
    crate::overlay::creation_output::validate_staging_path(
        &delivery.dispatch_id,
        output_name,
        Path::new(&delivery.staging_path),
    )?;
    Ok(())
}

fn verify_active_intent(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    request_fingerprint: &str,
    output_name: &str,
    staging_path: &str,
    output_path: &str,
) -> Result<(), String> {
    let assignment = crate::overlay::creation_intent_journal::verify_delivery_assignment(
        product,
        job_id,
        dispatch_id,
        request_fingerprint,
        output_name,
    )?;
    if !same_path(&assignment.staging_path.to_string_lossy(), staging_path)
        || publication::output_identity(&assignment.output_path)?
            != publication::output_identity(Path::new(output_path))?
    {
        return Err("Creation delivery conflicts with its accepted request.".to_string());
    }
    Ok(())
}

fn validate_cancellation(cancellation: &CancelledDelivery) -> Result<(), String> {
    validate_product(cancellation.product)?;
    if !valid_id(&cancellation.job_id)
        || !valid_id(&cancellation.dispatch_id)
        || !valid_fingerprint(&cancellation.request_fingerprint)
        || cancellation.output_name.is_empty()
        || cancellation.output_name.len() > 255
        || !valid_product_output_name(cancellation.product, &cancellation.output_name)
    {
        return Err("Creation cancellation state is invalid.".to_string());
    }
    crate::overlay::creation_output::assigned_path(Path::new("."), &cancellation.output_name)
        .map(|_| ())
}

fn cleanup_delivery_staging(
    dispatch_id: &str,
    output_name: &str,
    staging_path: &str,
) -> Result<(), String> {
    let staging = crate::overlay::creation_output::validate_staging_path(
        dispatch_id,
        output_name,
        Path::new(staging_path),
    )?;
    crate::overlay::creation_output::cleanup_staging(dispatch_id, output_name, &staging)
}

fn cleanup_record_staging(record: &DeliveryRecord) -> Result<(), String> {
    cleanup_delivery_staging(
        &record.dispatch_id,
        &record.output_name,
        &record.staging_path,
    )
}

fn validate_saved_delivery(
    saved: &DeliveryRecord,
    delivery: &PublishedDelivery,
    artifact: &crate::overlay::generation_history::DeliveryArtifactIdentity,
) -> Result<(), String> {
    if saved.product != delivery.product
        || saved.job_id != delivery.job_id
        || saved.request_fingerprint != delivery.request_fingerprint
        || saved.source_path != delivery.source_path
        || saved.output_name != delivery.output_name
        || !same_path(&saved.staging_path, &delivery.staging_path)
        || !same_path(&saved.output_path, &delivery.output_path)
        || saved.metadata != delivery.metadata
        || saved.artifact_size_bytes != artifact.size_bytes
        || saved.artifact_sha256 != artifact.sha256
    {
        return Err("Creation delivery conflicts with its saved state.".to_string());
    }
    Ok(())
}

fn validate_product(product: &str) -> Result<(), String> {
    matches!(product, "3d" | "svg" | "image")
        .then_some(())
        .ok_or_else(|| "Creation delivery product is invalid.".to_string())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_product_output_name(product: &str, output_name: &str) -> bool {
    matches!(
        (
            product,
            Path::new(output_name)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
        ),
        ("3d", Some("glb")) | ("svg", Some("svg")) | ("image", Some("png"))
    )
}

fn same_path(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
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
