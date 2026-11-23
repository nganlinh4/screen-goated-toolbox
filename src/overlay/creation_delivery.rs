//! Durable completion handoff for creation results.
//!
//! One user dispatch owns one published artifact and one history commit. A
//! restart replays only the unfinished handoff; it never equates equal content
//! with the same user action.

use crate::overlay::store_support::now_ms;
use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod cancellation;
mod companion;
#[cfg(test)]
mod cross_process;
mod input;
mod journal;
pub(crate) mod publication;
mod retry;

pub(crate) use cancellation::{
    CancellationOutcome, cancel_dispatch, cancel_product_intents, ensure_cancellation_capacity,
};
pub(crate) use companion::PublishedCompanion;
use input::*;
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
    pub companion: Option<PublishedCompanion>,
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
    #[serde(default)]
    companion: Option<companion::CompanionRecord>,
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

pub fn commit(mut delivery: PublishedDelivery) -> Result<(), String> {
    validate_input(&delivery)?;
    delivery.output_path = publication::output_identity(Path::new(&delivery.output_path))?;
    if let Some(companion) = delivery.companion.as_mut() {
        companion.output_path = publication::output_identity(Path::new(&companion.output_path))?;
    }
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
    let companion = companion::inspect(&delivery)?;
    let result = (|| {
        let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
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
        ensure_outputs_unreserved(
            &delivery.output_path,
            delivery
                .companion
                .as_ref()
                .map(|companion| companion.output_path.as_str()),
        )?;
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
            if store.entries.len() >= MAX_ENTRIES
                || store.entries.iter().any(|entry| {
                    entry.product == delivery.product && entry.job_id == delivery.job_id
                })
                || outputs_conflict(&store, &delivery)?
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
                companion: companion.map(|(value, artifact)| companion::saved(value, artifact)),
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
                    companion::metadata(record),
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

fn outputs_conflict(store: &DeliveryStore, delivery: &PublishedDelivery) -> Result<bool, String> {
    let mut proposed = vec![
        crate::overlay::creation_path::normalized_absolute(Path::new(&delivery.output_path))?
            .to_string_lossy()
            .to_string(),
    ];
    if let Some(companion) = &delivery.companion {
        let identity =
            crate::overlay::creation_path::normalized_absolute(Path::new(&companion.output_path))?
                .to_string_lossy()
                .to_string();
        if crate::overlay::creation_path::same(&proposed[0], &identity)? {
            return Ok(true);
        }
        proposed.push(identity);
    }
    for entry in &store.entries {
        for path in std::iter::once(entry.output_path.as_str()).chain(
            entry
                .companion
                .as_ref()
                .map(|companion| companion.output_path.as_str()),
        ) {
            let existing = crate::overlay::creation_path::normalized_absolute(Path::new(path))?
                .to_string_lossy()
                .to_string();
            for identity in &proposed {
                if crate::overlay::creation_path::same(&existing, identity)? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn ensure_outputs_unreserved(output: &str, companion: Option<&str>) -> Result<(), String> {
    crate::overlay::generation_history::ensure_output_unreserved(Path::new(output))?;
    if let Some(companion) = companion {
        crate::overlay::generation_history::ensure_output_unreserved(Path::new(companion))?;
    }
    Ok(())
}

pub fn reconcile_product(product: &str) -> Result<HashSet<String>, String> {
    validate_product(product)?;
    let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
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

mod reservations;
pub(crate) use reservations::*;

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
        if let Some(companion) = &record.companion {
            companion::cancel(companion)?;
        }
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
        if store.entries[index]
            .companion
            .as_ref()
            .is_some_and(|companion| companion.file_identity.is_none())
        {
            let identity = companion::reserve(
                store.entries[index]
                    .companion
                    .as_ref()
                    .expect("companion is present"),
            )?;
            store.entries[index]
                .companion
                .as_mut()
                .expect("companion is present")
                .file_identity = Some(identity);
            save(path, store)?;
        }
        if let Some(companion) = &store.entries[index].companion {
            companion::publish(companion)?;
        }
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
            // Hold the receipt open until its identity is durably saved.
            drop(reservation);
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
            .flat_map(|entry| {
                std::iter::once(entry.output_path.clone()).chain(
                    entry
                        .companion
                        .iter()
                        .map(|companion| companion.output_path.clone()),
                )
            })
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
    if let Some(companion) = &record.companion {
        companion::cleanup_staging(companion)?;
    }
    cleanup_delivery_staging(
        &record.dispatch_id,
        &record.output_name,
        &record.staging_path,
    )
}

#[cfg(test)]
mod tests;
