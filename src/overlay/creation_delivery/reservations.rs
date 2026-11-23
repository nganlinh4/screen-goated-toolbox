use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryOutcome {
    Pending,
    Delivered,
    Cancelled,
    Missing,
}

pub(crate) fn outcome(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    output_path: &str,
) -> Result<DeliveryOutcome, String> {
    validate_product(product)?;
    let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
    let store = load_store(&journal_path())?;
    if store.cancellations.iter().any(|item| {
        item.product == product && item.job_id == job_id && item.dispatch_id == dispatch_id
    }) {
        return Ok(DeliveryOutcome::Cancelled);
    }
    if store.entries.iter().any(|item| {
        item.product == product && item.job_id == job_id && item.dispatch_id == dispatch_id
    }) {
        return Ok(DeliveryOutcome::Pending);
    }
    if crate::overlay::generation_history::has_delivery_receipt(product, dispatch_id, output_path)?
    {
        Ok(DeliveryOutcome::Delivered)
    } else {
        Ok(DeliveryOutcome::Missing)
    }
}

pub(crate) struct PendingOutputReservation {
    pub output_path: String,
    pub file_identity: Option<String>,
}

pub(crate) fn pending_output_reservations() -> Result<Vec<PendingOutputReservation>, String> {
    let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
    Ok(load_store(&journal_path())?
        .entries
        .into_iter()
        .flat_map(|entry| {
            std::iter::once(PendingOutputReservation {
                output_path: entry.output_path,
                file_identity: entry.publication_file_identity,
            })
            .chain(entry.companion.into_iter().map(|companion| {
                PendingOutputReservation {
                    output_path: companion.output_path,
                    file_identity: companion.file_identity,
                }
            }))
        })
        .collect())
}

pub(crate) fn pending_source_paths() -> Result<HashSet<String>, String> {
    let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
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
    let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
    load_store(&journal_path())?
        .entries
        .into_iter()
        .map(storage_reservation)
        .collect()
}

pub(super) fn storage_reservation(
    entry: DeliveryRecord,
) -> Result<PendingStorageReservation, String> {
    let additional_output_bytes = if entry.stage == DeliveryStage::Validated {
        entry
            .artifact_size_bytes
            .checked_add(
                entry
                    .companion
                    .as_ref()
                    .map_or(0, |side| side.artifact_size_bytes),
            )
            .ok_or("Creation storage reservation is too large.")?
    } else {
        0
    };
    Ok(PendingStorageReservation {
        dispatch_id: entry.dispatch_id,
        output_path: entry.output_path,
        additional_output_bytes,
    })
}

pub(crate) fn protected_dispatch_ids() -> Result<HashSet<String>, String> {
    let _guard = crate::overlay::creation_lifecycle::acquire_delivery()?;
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
