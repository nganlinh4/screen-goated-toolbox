use std::collections::HashSet;
use std::path::Path;

use super::{CancellationRecord, CancelledDelivery, DeliveryStore};

pub(crate) fn cancel_dispatch(cancellation: CancelledDelivery) -> Result<(), String> {
    super::validate_cancellation(&cancellation)?;
    let product = cancellation.product;
    let _guard = super::DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    let assignment = crate::overlay::creation_intent_journal::verify_delivery_assignment(
        cancellation.product,
        &cancellation.job_id,
        &cancellation.dispatch_id,
        &cancellation.request_fingerprint,
        &cancellation.output_name,
    )?;
    let expected_staging = crate::overlay::creation_output::staging_path(
        &cancellation.dispatch_id,
        &cancellation.output_name,
    )?;
    if !super::same_path(
        &assignment.staging_path.to_string_lossy(),
        &expected_staging.to_string_lossy(),
    ) {
        return Err("Creation cancellation conflicts with its accepted request.".to_string());
    }
    let path = super::journal_path();
    let mut store = super::load_store(&path)?;
    prune_cancellations(&mut store, super::now_ms());
    if store.entries.iter().any(|entry| {
        entry.dispatch_id == cancellation.dispatch_id && !entry.stage.is_pre_publication()
    }) {
        return advance_saved_delivery(&path, &mut store, &cancellation.dispatch_id);
    }
    if let Some(existing) = store
        .cancellations
        .iter()
        .find(|existing| existing.dispatch_id == cancellation.dispatch_id)
    {
        if existing.product != cancellation.product
            || existing.job_id != cancellation.job_id
            || existing.request_fingerprint != cancellation.request_fingerprint
            || existing.output_name != cancellation.output_name
        {
            return Err("Creation cancellation conflicts with its saved state.".to_string());
        }
    } else {
        if store.cancellations.len() >= super::MAX_CANCELLATIONS
            || store.cancellations.iter().any(|existing| {
                existing.product == cancellation.product && existing.job_id == cancellation.job_id
            })
            || store.entries.iter().any(|entry| {
                entry.dispatch_id == cancellation.dispatch_id && !entry.stage.is_pre_publication()
            })
        {
            return Err("Creation cancellation is unavailable after publication.".to_string());
        }
        let created_at_ms = super::now_ms();
        store.cancellations.push(CancellationRecord {
            product: cancellation.product.to_string(),
            job_id: cancellation.job_id,
            dispatch_id: cancellation.dispatch_id,
            request_fingerprint: cancellation.request_fingerprint,
            output_name: cancellation.output_name,
            created_at_ms,
            expires_at_ms: created_at_ms.saturating_add(super::CANCELLATION_LIFETIME_MS),
        });
        super::save_store(&path, &store)?;
    }
    let cleanup_error = finalize_cancellations(&path, &mut store, product).err();
    drop(_guard);
    if let Some(error) = cleanup_error {
        crate::log_info!("[Creation delivery] Cancellation cleanup is still pending: {error}");
        super::schedule_reconciliation(product);
    }
    Ok(())
}

pub(crate) fn cancel_product_intents(product: &'static str) -> bool {
    if super::validate_product(product).is_err() {
        return false;
    }
    let intents = match crate::overlay::creation_intent_journal::load(product) {
        Ok(intents) => intents,
        Err(error) => {
            crate::log_info!("[Creation delivery] Saved cancellation work is unavailable: {error}");
            return false;
        }
    };
    let mut durable = true;
    for intent in intents {
        let Some(output_name) = intent
            .arguments
            .get("outputName")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            durable &=
                crate::overlay::creation_intent_journal::clear_required(product, &intent.job_id)
                    .is_ok();
            continue;
        };
        durable &= cancel_dispatch(CancelledDelivery {
            product,
            job_id: intent.job_id,
            dispatch_id: intent.dispatch_id,
            request_fingerprint: intent.arguments_fingerprint,
            output_name: output_name.to_string(),
        })
        .is_ok();
    }
    durable
}

pub(crate) fn ensure_cancellation_capacity() -> Result<(), String> {
    let active_jobs = crate::overlay::creation_intent_journal::load_all()?.len();
    let _guard = super::DELIVERY_LOCK
        .lock()
        .map_err(|_| "Creation delivery state is unavailable.".to_string())?;
    let path = super::journal_path();
    let mut store = super::load_store(&path)?;
    if prune_cancellations(&mut store, super::now_ms()) {
        super::save_store(&path, &store)?;
    }
    if store
        .cancellations
        .len()
        .saturating_add(active_jobs)
        .saturating_add(1)
        > super::MAX_CANCELLATIONS
        || (serde_json::to_vec(&store)
            .map_err(|_| "Creation cancellation capacity is unavailable.".to_string())?
            .len() as u64)
            .saturating_add(
                active_jobs.saturating_add(1) as u64 * super::MAX_SERIALIZED_CANCELLATION_BYTES,
            )
            > super::MAX_FILE_BYTES
    {
        return Err("Creation cancellation capacity is unavailable.".to_string());
    }
    Ok(())
}

pub(super) fn advance_saved_delivery(
    path: &Path,
    store: &mut DeliveryStore,
    dispatch_id: &str,
) -> Result<(), String> {
    super::advance_delivery(
        path,
        store,
        dispatch_id,
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
            crate::overlay::creation_intent_journal::clear_required(&record.product, &record.job_id)
        },
        super::save_store,
    )
}

pub(super) fn pending_job_ids(store: &DeliveryStore, product: &str) -> HashSet<String> {
    store
        .entries
        .iter()
        .filter(|entry| entry.product == product)
        .map(|entry| entry.job_id.clone())
        .chain(
            store
                .cancellations
                .iter()
                .filter(|entry| entry.product == product)
                .map(|entry| entry.job_id.clone()),
        )
        .collect()
}

pub(super) fn finalize_cancellations(
    path: &Path,
    store: &mut DeliveryStore,
    product: &str,
) -> Result<(), String> {
    let cancellations = store
        .cancellations
        .iter()
        .filter(|cancellation| cancellation.product == product)
        .cloned()
        .collect::<Vec<_>>();
    let mut changed = prune_cancellations(store, super::now_ms());
    let mut pending = false;
    for cancellation in cancellations {
        if let Some(record) = store
            .entries
            .iter()
            .find(|entry| entry.dispatch_id == cancellation.dispatch_id)
            .cloned()
            && super::publication::cancel_pre_publication(&record).is_err()
        {
            pending = true;
            continue;
        }
        if crate::overlay::creation_intent_journal::clear_required(
            &cancellation.product,
            &cancellation.job_id,
        )
        .is_err()
        {
            pending = true;
            continue;
        }
        let staging = crate::overlay::creation_output::staging_path(
            &cancellation.dispatch_id,
            &cancellation.output_name,
        )?;
        if crate::overlay::creation_output::cleanup_staging(
            &cancellation.dispatch_id,
            &cancellation.output_name,
            &staging,
        )
        .is_err()
        {
            pending = true;
            continue;
        }
        let before = store.entries.len();
        store.entries.retain(|entry| {
            entry.dispatch_id != cancellation.dispatch_id || !entry.stage.is_pre_publication()
        });
        changed |= store.entries.len() != before;
        changed |= retire(store, &cancellation.dispatch_id);
    }
    if changed {
        super::save_store(path, store)?;
    }
    if pending {
        Err("Cancelled creation cleanup is still pending.".to_string())
    } else {
        Ok(())
    }
}

pub(super) fn retire(store: &mut DeliveryStore, dispatch_id: &str) -> bool {
    let before = store.cancellations.len();
    store
        .cancellations
        .retain(|cancellation| cancellation.dispatch_id != dispatch_id);
    store.cancellations.len() != before
}

fn prune_cancellations(store: &mut DeliveryStore, now: u64) -> bool {
    let before = store.cancellations.len();
    let active_dispatches = store
        .entries
        .iter()
        .map(|entry| entry.dispatch_id.as_str())
        .collect::<HashSet<_>>();
    store.cancellations.retain(|cancellation| {
        cancellation.expires_at_ms > now
            || active_dispatches.contains(cancellation.dispatch_id.as_str())
    });
    store.cancellations.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_cancellation_blocks_recovery_even_without_a_delivery_entry() {
        let mut store = DeliveryStore::default();
        store.cancellations.push(CancellationRecord {
            product: "image".to_string(),
            job_id: "image_job".to_string(),
            dispatch_id: "image_dispatch".to_string(),
            request_fingerprint: "a".repeat(64),
            output_name: "result.png".to_string(),
            created_at_ms: 1,
            expires_at_ms: 2,
        });
        assert_eq!(
            pending_job_ids(&store, "image"),
            HashSet::from(["image_job".to_string()])
        );
        assert!(pending_job_ids(&store, "svg").is_empty());
    }

    #[test]
    fn completed_cancellation_retires_only_its_tombstone() {
        let cancellation = |job: &str, dispatch: &str| CancellationRecord {
            product: "image".to_string(),
            job_id: job.to_string(),
            dispatch_id: dispatch.to_string(),
            request_fingerprint: "a".repeat(64),
            output_name: format!("{job}.png"),
            created_at_ms: 1,
            expires_at_ms: 2,
        };
        let mut store = DeliveryStore {
            cancellations: vec![
                cancellation("image_job_a", "image_dispatch_a"),
                cancellation("image_job_b", "image_dispatch_b"),
            ],
            ..DeliveryStore::default()
        };
        assert!(retire(&mut store, "image_dispatch_a"));
        assert_eq!(store.cancellations.len(), 1);
        assert_eq!(store.cancellations[0].dispatch_id, "image_dispatch_b");
        assert!(!retire(&mut store, "image_dispatch_missing"));
    }
}
