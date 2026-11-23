use crate::component_registry::is_reparse_point;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use super::*;

mod reclaim;
mod storage_scan;
mod volume;
use reclaim::*;
use storage_scan::*;
use volume::*;
const FREE_SPACE_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RELATIVE_FREE_RESERVE_BYTES: u64 = FREE_SPACE_RESERVE_BYTES;
const FREE_PRESSURE_HIGH_BYTES: u64 = 512 * 1024 * 1024;
const FREE_PRESSURE_LOW_BYTES: u64 = 1024 * 1024 * 1024;
const MANAGED_HIGH_WATERMARK_BYTES: u64 = MAX_MANAGED_ARTIFACT_BYTES * 9 / 10;
const MANAGED_LOW_WATERMARK_BYTES: u64 = MAX_MANAGED_ARTIFACT_BYTES * 4 / 5;
const MAX_SCANNED_MANAGED_ENTRIES: usize = 16_384;
static ADMISSION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Copy, Debug)]
struct Budget {
    enforce_managed_cap: bool,
    managed_bytes: u64,
    pending_managed_bytes: u64,
    available_bytes: u64,
    free_reserve_bytes: u64,
    pending_volume_bytes: u64,
    requested_volume_bytes: u64,
    requested_managed_bytes: u64,
    reclaimable_bytes: u64,
}

fn required_reclaim(budget: Budget) -> Result<u64, String> {
    let reclaimable_bytes = if budget.enforce_managed_cap {
        budget.reclaimable_bytes
    } else {
        0
    };
    let free_required = u128::from(budget.free_reserve_bytes)
        .saturating_add(u128::from(budget.pending_volume_bytes))
        .saturating_add(u128::from(budget.requested_volume_bytes))
        .saturating_sub(u128::from(budget.available_bytes));
    let managed_required = if budget.enforce_managed_cap {
        u128::from(budget.managed_bytes)
            .saturating_add(u128::from(budget.pending_managed_bytes))
            .saturating_add(u128::from(budget.requested_managed_bytes))
            .saturating_sub(u128::from(MAX_MANAGED_ARTIFACT_BYTES))
    } else {
        0
    };
    let hard_required = free_required.max(managed_required);
    if hard_required > u128::from(reclaimable_bytes) || hard_required > u128::from(u64::MAX) {
        return Err("Creation storage does not have enough available space.".to_string());
    }
    let post_free = u128::from(budget.available_bytes).saturating_sub(
        u128::from(budget.pending_volume_bytes)
            .saturating_add(u128::from(budget.requested_volume_bytes)),
    );
    let free_pressure = if post_free
        < u128::from(
            budget
                .free_reserve_bytes
                .saturating_add(FREE_PRESSURE_HIGH_BYTES),
        ) {
        u128::from(
            budget
                .free_reserve_bytes
                .saturating_add(FREE_PRESSURE_LOW_BYTES),
        )
        .saturating_sub(post_free)
    } else {
        0
    };
    let managed_after = u128::from(budget.managed_bytes)
        .saturating_add(u128::from(budget.pending_managed_bytes))
        .saturating_add(u128::from(budget.requested_managed_bytes));
    let managed_pressure =
        if budget.enforce_managed_cap && managed_after > u128::from(MANAGED_HIGH_WATERMARK_BYTES) {
            managed_after.saturating_sub(u128::from(MANAGED_LOW_WATERMARK_BYTES))
        } else {
            0
        };
    let target = hard_required
        .max(free_pressure)
        .max(managed_pressure)
        .min(u128::from(reclaimable_bytes));
    Ok(target as u64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reservation {
    output_bytes: u64,
    internal_bytes: u64,
}

fn product_reservation(
    product: &str,
    source_bytes: u64,
    source_count: usize,
) -> Result<Reservation, String> {
    let source_limit = match product {
        "3d" | "svg" => crate::overlay::creation_source::MAX_SOURCE_BYTES,
        "image" => IMAGE_REFERENCE_RESERVATION_BYTES,
        _ => return Err("Creation storage request is invalid.".to_string()),
    };
    if source_bytes > source_limit
        || (source_bytes == 0) != (source_count == 0)
        || (matches!(product, "3d" | "svg") && source_count != 1)
        || (product == "image" && source_count > 20)
    {
        return Err("Creation source storage request is invalid.".to_string());
    }
    let presentation_bytes = SOURCE_PRESENTATION_RESERVATION_BYTES
        .checked_mul(source_count as u64)
        .ok_or_else(|| "Creation source storage request is invalid.".to_string())?;
    match product {
        "3d" => Ok(Reservation {
            output_bytes: THREE_D_RESULT_RESERVATION_BYTES,
            internal_bytes: THREE_D_RESULT_RESERVATION_BYTES
                .saturating_add(source_bytes)
                .saturating_add(presentation_bytes),
        }),
        "svg" => Ok(Reservation {
            output_bytes: SVG_RESULT_RESERVATION_BYTES,
            internal_bytes: SVG_RESULT_RESERVATION_BYTES
                .saturating_add(source_bytes)
                .saturating_add(presentation_bytes),
        }),
        "image" => Ok(Reservation {
            output_bytes: IMAGE_RESULT_RESERVATION_BYTES,
            internal_bytes: IMAGE_RESULT_RESERVATION_BYTES
                .saturating_add(source_bytes)
                .saturating_add(presentation_bytes),
        }),
        _ => unreachable!(),
    }
}

fn intent_source_reservation(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<(u64, Vec<crate::overlay::creation_source::SourceDescriptor>), String> {
    let Some(descriptors) = intent.arguments.get("sourceDescriptors") else {
        return if intent.product == "image" {
            Ok((0, Vec::new()))
        } else {
            Err("Saved creation source reservation is invalid.".to_string())
        };
    };
    let descriptors: Vec<crate::overlay::creation_source::SourceDescriptor> =
        serde_json::from_value(descriptors.clone())
            .map_err(|_| "Saved creation source reservation is invalid.".to_string())?;
    let bytes = descriptors.iter().try_fold(0_u64, |total, descriptor| {
        total
            .checked_add(descriptor.size_bytes)
            .ok_or_else(|| "Saved creation source reservation is invalid.".to_string())
    })?;
    Ok((bytes, descriptors))
}

#[cfg(test)]
fn requested_root_bytes(reservation: Reservation, output_uses_root: bool) -> u64 {
    reservation
        .internal_bytes
        .saturating_add(if output_uses_root {
            reservation.output_bytes
        } else {
            0
        })
}

fn path_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_ascii_lowercase()
}

fn saved_path_key(path: &Path) -> Result<String, String> {
    crate::overlay::creation_path::normalized_absolute(path)
        .map(|path| path.to_string_lossy().to_string())
}

fn free_reserve_bytes(total_bytes: u64) -> u64 {
    FREE_SPACE_RESERVE_BYTES.max((total_bytes / 100).min(MAX_RELATIVE_FREE_RESERVE_BYTES))
}

fn managed_root() -> Result<PathBuf, String> {
    canonical_managed_root(crate::paths::app_runtime_local_data_dir())
}

fn runtime_managed_root() -> Result<PathBuf, String> {
    canonical_managed_root(crate::paths::app_runtime_local_data_dir())
}

fn canonical_managed_root(root: PathBuf) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&root)
        .map_err(|_| "Creation storage location is unavailable.".to_string())?;
    std::fs::canonicalize(root).map_err(|_| "Creation storage location is unavailable.".to_string())
}

fn is_managed_output_directory(path: &Path, root: &Path) -> Result<bool, String> {
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    if !crate::overlay::creation_path::same(&saved_path_key(parent)?, &saved_path_key(root)?)? {
        return Ok(false);
    }
    let Some(name) = path.file_name() else {
        return Ok(false);
    };
    for managed in ["3d-generator", "vectors", "images"] {
        if crate::overlay::creation_path::same(&name.to_string_lossy(), managed)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn intent_directories(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<(PathBuf, PathBuf), String> {
    let staging = intent
        .arguments
        .get("outputDir")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "Saved creation storage request is invalid.".to_string())?;
    let output = intent
        .arguments
        .get("finalOutputDir")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "Saved creation storage request is invalid.".to_string())?;
    crate::overlay::creation_intent_journal::validate_persisted_path(&staging)?;
    crate::overlay::creation_intent_journal::validate_persisted_path(&output)?;
    crate::overlay::creation_path::normalized_absolute(&staging)?;
    crate::overlay::creation_path::normalized_absolute(&output)?;
    Ok((staging, output))
}

fn intent_expected_path(
    intent: &crate::overlay::creation_intent_journal::Intent,
    directory: &Path,
) -> Option<PathBuf> {
    let name = intent.arguments.get("outputName")?.as_str()?;
    crate::overlay::creation_output::assigned_path(directory, name).ok()
}

struct ReservationSnapshot {
    pending_volume_bytes: HashMap<String, u64>,
    pending_managed_bytes: u64,
    protected_paths: HashSet<String>,
}

fn add_volume_reservation(
    reservations: &mut HashMap<String, u64>,
    volume: String,
    bytes: u64,
) -> Result<(), String> {
    let mut matching = None;
    for key in reservations.keys() {
        if crate::overlay::creation_path::same(key, &volume)? {
            matching = Some(key.clone());
            break;
        }
    }
    let total = reservations.entry(matching.unwrap_or(volume)).or_default();
    *total = total
        .checked_add(bytes)
        .ok_or_else(|| "Creation storage reservation is too large.".to_string())?;
    Ok(())
}

fn reservation_snapshot(
    root: &Path,
    internal_volume: &str,
    pending_deliveries: &[crate::overlay::creation_delivery::PendingStorageReservation],
) -> Result<ReservationSnapshot, String> {
    reservation_snapshot_for(
        &crate::overlay::creation_intent_journal::load_all()?,
        root,
        internal_volume,
        pending_deliveries,
    )
}

fn reservation_snapshot_for(
    intents: &[crate::overlay::creation_intent_journal::Intent],
    root: &Path,
    internal_volume: &str,
    pending_deliveries: &[crate::overlay::creation_delivery::PendingStorageReservation],
) -> Result<ReservationSnapshot, String> {
    let mut deliveries = pending_deliveries
        .iter()
        .map(|delivery| (delivery.dispatch_id.as_str(), delivery))
        .collect::<HashMap<_, _>>();
    let mut snapshot = ReservationSnapshot {
        pending_volume_bytes: HashMap::new(),
        pending_managed_bytes: 0,
        protected_paths: pending_deliveries
            .iter()
            .map(|delivery| saved_path_key(Path::new(&delivery.output_path)))
            .collect::<Result<_, _>>()?,
    };
    for intent in intents {
        let (staging_directory, output_directory) = intent_directories(intent)?;
        let expected = intent_expected_path(intent, &output_directory);
        let staging = intent_expected_path(intent, &staging_directory);
        let expected_key = expected
            .as_ref()
            .map(|path| saved_path_key(path))
            .transpose()?;
        if let Some(path) = &expected_key {
            snapshot.protected_paths.insert(path.clone());
        }
        let (source_bytes, descriptors) = intent_source_reservation(intent)?;
        let reservation = product_reservation(&intent.product, source_bytes, descriptors.len())?;
        crate::overlay::creation_source_snapshot::validate_sources(&descriptors)?;
        let remaining_presentations =
            crate::overlay::creation_source_snapshot::remaining_presentation_reservation(
                &descriptors,
            )?;
        let delivery = deliveries.remove(intent.dispatch_id.as_str());
        if let (Some(delivery), Some(expected_key)) = (delivery, expected_key.as_ref())
            && !crate::overlay::creation_path::same(
                &saved_path_key(Path::new(&delivery.output_path))?,
                expected_key,
            )?
        {
            return Err("Saved creation storage request conflicts with delivery.".to_string());
        }
        let existing_staging_bytes = staging
            .as_ref()
            .and_then(|path| std::fs::symlink_metadata(path).ok())
            .filter(|metadata| metadata.file_type().is_file() && !is_reparse_point(metadata))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let remaining_output = delivery
            .map(|delivery| delivery.additional_output_bytes)
            .unwrap_or(reservation.output_bytes);
        let remaining_internal = delivery.map(|_| 0).unwrap_or_else(|| {
            reservation
                .output_bytes
                .saturating_sub(existing_staging_bytes)
                .saturating_add(remaining_presentations)
        });
        add_volume_reservation(
            &mut snapshot.pending_volume_bytes,
            volume_key(&output_directory)?,
            remaining_output,
        )?;
        add_volume_reservation(
            &mut snapshot.pending_volume_bytes,
            internal_volume.to_string(),
            remaining_internal,
        )?;
        snapshot.pending_managed_bytes = snapshot
            .pending_managed_bytes
            .checked_add(remaining_internal)
            .ok_or_else(|| "Creation storage reservation is too large.".to_string())?;
        if is_managed_output_directory(&output_directory, root)? {
            snapshot.pending_managed_bytes = snapshot
                .pending_managed_bytes
                .checked_add(remaining_output)
                .ok_or_else(|| "Creation storage reservation is too large.".to_string())?;
        }
    }
    for delivery in deliveries.into_values() {
        let directory = Path::new(&delivery.output_path)
            .parent()
            .ok_or("Saved creation storage destination is invalid.")?;
        add_volume_reservation(
            &mut snapshot.pending_volume_bytes,
            volume_key(directory)?,
            delivery.additional_output_bytes,
        )?;
        if is_managed_output_directory(directory, root)? {
            snapshot.pending_managed_bytes = snapshot
                .pending_managed_bytes
                .checked_add(delivery.additional_output_bytes)
                .ok_or("Creation storage reservation is too large.")?;
        }
    }
    Ok(snapshot)
}

fn run_saved_cleanup(
    path: &Path,
    store: &mut ResultHistoryStore,
    protected: &protected::ProtectedPaths,
) -> Result<(), String> {
    if prepare_cleanup_quarantines(store) {
        save_store(path, store)?;
    }
    if run_pending_cleanup_at_protected(path, store, protected)? {
        retain_live_delivery_identities(store);
        save_store(path, store)?;
    }
    if deletion::reconcile_pending_deletes_protected(store, protected) {
        save_store(path, store)?;
    }
    Ok(())
}

fn ensure_capacity(
    output_dir: &Path,
    requested: Reservation,
    pending_deliveries: Vec<crate::overlay::creation_delivery::PendingStorageReservation>,
    pending_paths: &protected::ProtectedPaths,
    results_per_tool: usize,
) -> Result<(), String> {
    let output_volume = volume_key(output_dir)?;
    let root = managed_root()?;
    let runtime_root = runtime_managed_root()?;
    let root_volume = volume_key(&root)?;
    let runtime_volume = volume_key(&runtime_root)?;
    let managed = is_managed_output_directory(output_dir, &root)?;
    let reservations = reservation_snapshot(&root, &runtime_volume, &pending_deliveries)?;
    let pending_on = |volume: &str| volume_bytes(&reservations.pending_volume_bytes, volume);
    let mut requested_by_volume = HashMap::new();
    add_volume_reservation(
        &mut requested_by_volume,
        runtime_volume.clone(),
        requested.internal_bytes,
    )?;
    add_volume_reservation(
        &mut requested_by_volume,
        output_volume.clone(),
        requested.output_bytes,
    )?;
    for (volume, bytes) in &requested_by_volume {
        if crate::overlay::creation_path::same(volume, &root_volume)? {
            continue;
        }
        let probe = if crate::overlay::creation_path::same(volume, &runtime_volume)? {
            runtime_root.as_path()
        } else {
            output_dir
        };
        let (available, total) = volume_capacity(probe)?;
        required_reclaim(Budget {
            enforce_managed_cap: false,
            managed_bytes: 0,
            pending_managed_bytes: 0,
            available_bytes: available,
            free_reserve_bytes: free_reserve_bytes(total),
            pending_volume_bytes: pending_on(volume)?,
            requested_volume_bytes: *bytes,
            requested_managed_bytes: 0,
            reclaimable_bytes: 0,
        })?;
    }

    let path = history_path();
    let history_guard = locking::acquire()?;
    let mut store = load_store(&path)?;
    let mut protected = protected::ProtectedPaths::from_saved_paths(
        reservations.protected_paths.iter().map(PathBuf::from),
    )?;
    protected.extend(pending_paths);
    let protected_paths = protected.expanded(&store)?;
    reconcile_store_protected(&path, &mut store, results_per_tool, &protected_paths)?;
    run_saved_cleanup(&path, &mut store, &protected)?;

    let roots = if root == runtime_root {
        vec![root.clone()]
    } else {
        vec![root.clone(), runtime_root.clone()]
    };
    let mut files = scan_managed_files(&roots)?;
    let root_requested_volume = volume_bytes(&requested_by_volume, &root_volume)?;
    let root_requested_managed =
        requested
            .internal_bytes
            .saturating_add(if managed { requested.output_bytes } else { 0 });
    let (root_available, root_total) = volume_capacity(&root)?;
    let budget = Budget {
        enforce_managed_cap: true,
        managed_bytes: files.total_bytes(),
        pending_managed_bytes: reservations.pending_managed_bytes,
        available_bytes: root_available,
        free_reserve_bytes: free_reserve_bytes(root_total),
        pending_volume_bytes: pending_on(&root_volume)?,
        requested_volume_bytes: root_requested_volume,
        requested_managed_bytes: root_requested_managed,
        reclaimable_bytes: 0,
    };
    let selected_ids = plan_reclamation(&store, &files.paths, &protected_paths, budget)?;
    let mut pruned_history = false;
    if !selected_ids.is_empty() {
        let mut index = store.entries.len();
        while index > 0 {
            index -= 1;
            if selected_ids.contains(&store.entries[index].id) {
                let entry = store.entries.remove(index);
                if !queue_managed_cleanup(&mut store, entry.clone()) {
                    store.entries.push(entry);
                }
            }
        }
        retain_live_delivery_identities(&mut store);
        prepare_cleanup_quarantines(&mut store);
        save_store(&path, &store)?;
        run_saved_cleanup(&path, &mut store, &protected)?;
        pruned_history = true;
    }
    drop(history_guard);
    if pruned_history {
        crate::overlay::creation_source_snapshot::sweep_pressure()?;
        files = scan_managed_files(&roots)?;
    }
    let (root_available, root_total) = volume_capacity(&root)?;
    required_reclaim(Budget {
        enforce_managed_cap: true,
        managed_bytes: files.total_bytes(),
        pending_managed_bytes: reservations.pending_managed_bytes,
        available_bytes: root_available,
        free_reserve_bytes: free_reserve_bytes(root_total),
        pending_volume_bytes: pending_on(&root_volume)?,
        requested_volume_bytes: root_requested_volume,
        requested_managed_bytes: root_requested_managed,
        reclaimable_bytes: 0,
    })?;
    Ok(())
}

pub(crate) fn admit_and_record<T>(
    product: &str,
    output_dir: &Path,
    source_bytes: u64,
    source_count: usize,
    record: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _lifecycle = crate::overlay::creation_lifecycle::acquire()?;
    let requested = product_reservation(product, source_bytes, source_count)?;
    let _guard = ADMISSION_LOCK
        .lock()
        .map_err(|_| "Creation storage admission is unavailable.".to_string())?;
    crate::overlay::creation_source_snapshot::sweep_pressure()?;
    crate::overlay::creation_output::sweep_staging()?;
    crate::overlay::creation_delivery::ensure_cancellation_capacity()?;
    let results_per_tool = results_per_tool_limit();
    let _delivery = crate::overlay::creation_lifecycle::acquire_delivery()?;
    let pending = crate::overlay::creation_delivery::pending_storage_reservations()?;
    let pending_paths = protected::ProtectedPaths::pending()?;
    ensure_capacity(
        output_dir,
        requested,
        pending,
        &pending_paths,
        results_per_tool,
    )?;
    record()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "admission/destination_tests.rs"]
mod destination_tests;
