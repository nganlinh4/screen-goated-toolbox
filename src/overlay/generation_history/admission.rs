use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use super::*;

mod storage_scan;
use storage_scan::*;

const FREE_SPACE_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RELATIVE_FREE_RESERVE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
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

fn volume_key(path: &Path) -> Result<String, String> {
    let path = std::fs::canonicalize(path)
        .map_err(|_| "Creation storage location is unavailable.".to_string())?;
    match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            Ok(prefix.as_os_str().to_string_lossy().to_ascii_lowercase())
        }
        _ => Err("Creation storage location is unavailable.".to_string()),
    }
}

fn volume_capacity(path: &Path) -> Result<(u64, u64), String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut available = 0_u64;
    let mut total = 0_u64;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(path.as_ptr()),
            Some(&mut available),
            Some(&mut total),
            None,
        )
    }
    .map_err(|_| "Creation storage capacity is unavailable.".to_string())?;
    Ok((available, total))
}

fn free_reserve_bytes(total_bytes: u64) -> u64 {
    FREE_SPACE_RESERVE_BYTES.max((total_bytes / 50).min(MAX_RELATIVE_FREE_RESERVE_BYTES))
}

fn managed_root() -> Result<PathBuf, String> {
    canonical_managed_root(crate::paths::app_local_data_dir())
}

fn runtime_managed_root() -> Result<PathBuf, String> {
    canonical_managed_root(crate::paths::app_runtime_local_data_dir())
}

fn canonical_managed_root(root: PathBuf) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&root)
        .map_err(|_| "Creation storage location is unavailable.".to_string())?;
    std::fs::canonicalize(root).map_err(|_| "Creation storage location is unavailable.".to_string())
}

fn is_managed_output_directory(path: &Path, root: &Path) -> bool {
    path.parent() == Some(root)
        && path.file_name().is_some_and(|name| {
            matches!(
                name.to_string_lossy().as_ref(),
                "3d-generator" | "vectors" | "images"
            )
        })
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
    let staging = std::fs::canonicalize(staging)
        .map_err(|_| "Saved creation storage location is unavailable.".to_string())?;
    let output = std::fs::canonicalize(output)
        .map_err(|_| "Saved creation storage location is unavailable.".to_string())?;
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
    let total = reservations.entry(volume).or_default();
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
    let deliveries = pending_deliveries
        .iter()
        .map(|delivery| (delivery.dispatch_id.as_str(), delivery))
        .collect::<HashMap<_, _>>();
    let mut snapshot = ReservationSnapshot {
        pending_volume_bytes: HashMap::new(),
        pending_managed_bytes: 0,
        protected_paths: pending_deliveries
            .iter()
            .map(|delivery| path_key(Path::new(&delivery.output_path)))
            .collect(),
    };
    for intent in crate::overlay::creation_intent_journal::load_all()? {
        let (staging_directory, output_directory) = intent_directories(&intent)?;
        let expected = intent_expected_path(&intent, &output_directory);
        let staging = intent_expected_path(&intent, &staging_directory);
        let expected_key = expected.as_ref().map(|path| path_key(path));
        if let Some(path) = &expected_key {
            snapshot.protected_paths.insert(path.clone());
        }
        let (source_bytes, descriptors) = intent_source_reservation(&intent)?;
        let reservation = product_reservation(&intent.product, source_bytes, descriptors.len())?;
        crate::overlay::creation_source_snapshot::validate_sources(&descriptors)?;
        let remaining_presentations =
            crate::overlay::creation_source_snapshot::remaining_presentation_reservation(
                &descriptors,
            )?;
        let delivery = deliveries.get(intent.dispatch_id.as_str()).copied();
        if let (Some(delivery), Some(expected_key)) = (delivery, expected_key.as_ref())
            && path_key(Path::new(&delivery.output_path)) != *expected_key
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
        if is_managed_output_directory(&output_directory, root) {
            snapshot.pending_managed_bytes = snapshot
                .pending_managed_bytes
                .checked_add(remaining_output)
                .ok_or_else(|| "Creation storage reservation is too large.".to_string())?;
        }
    }
    Ok(snapshot)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn reclaimable_entries(
    store: &ResultHistoryStore,
    files: &HashMap<String, u64>,
    protected_paths: &HashSet<String>,
) -> Vec<ResultHistoryEntry> {
    let mut newest_by_tool: HashMap<&str, &ResultHistoryEntry> = HashMap::new();
    for entry in &store.entries {
        let replace = newest_by_tool
            .get(entry.tool.as_str())
            .is_none_or(|newest| {
                (entry.created_at_ms, entry.id.as_str())
                    > (newest.created_at_ms, newest.id.as_str())
            });
        if replace {
            newest_by_tool.insert(entry.tool.as_str(), entry);
        }
    }
    let protected_ids = newest_by_tool
        .values()
        .map(|entry| entry.id.as_str())
        .collect::<HashSet<_>>();
    let mut entries = store
        .entries
        .iter()
        .filter(|entry| {
            entry.managed_artifact
                && entry.artifact_size_bytes > 0
                && entry.artifact_sha256.len() == 64
                && !protected_ids.contains(entry.id.as_str())
                && !protected_paths.contains(&path_key(Path::new(&entry.output_path)))
                && files.get(&path_key(Path::new(&entry.output_path)))
                    == Some(&entry.artifact_size_bytes)
        })
        .cloned()
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.created_at_ms);
    entries
}

fn run_saved_cleanup(path: &Path, store: &mut ResultHistoryStore) -> Result<(), String> {
    if prepare_cleanup_quarantines(store) {
        save_store(path, store)?;
    }
    if run_pending_cleanup_at(path, store)? {
        retain_live_delivery_identities(store);
        save_store(path, store)?;
    }
    Ok(())
}

fn ensure_capacity(
    output_dir: &Path,
    requested: Reservation,
    pending_deliveries: Vec<crate::overlay::creation_delivery::PendingStorageReservation>,
) -> Result<(), String> {
    let output_volume = volume_key(output_dir)?;
    let root = managed_root()?;
    let runtime_root = runtime_managed_root()?;
    let root_volume = volume_key(&root)?;
    let runtime_volume = volume_key(&runtime_root)?;
    let managed = is_managed_output_directory(output_dir, &root);
    let reservations = reservation_snapshot(&root, &runtime_volume, &pending_deliveries)?;
    let pending_on = |volume: &str| {
        reservations
            .pending_volume_bytes
            .get(volume)
            .copied()
            .unwrap_or(0)
    };
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
        if volume == &root_volume {
            continue;
        }
        let probe = if volume == &runtime_volume {
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
            pending_volume_bytes: pending_on(volume),
            requested_volume_bytes: *bytes,
            requested_managed_bytes: 0,
            reclaimable_bytes: 0,
        })?;
    }

    let results_per_tool = results_per_tool_limit();
    let path = history_path();
    let history_guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    let mut store = load_store(&path)?;
    reconcile_store_protected(
        &path,
        &mut store,
        results_per_tool,
        &reservations.protected_paths,
    )?;
    run_saved_cleanup(&path, &mut store)?;

    let roots = if root == runtime_root {
        vec![root.clone()]
    } else {
        vec![root.clone(), runtime_root.clone()]
    };
    let mut files = scan_managed_files(&roots)?;
    let candidates = reclaimable_entries(&store, &files.paths, &reservations.protected_paths);
    let reclaimable_bytes = candidates
        .iter()
        .map(|entry| entry.artifact_size_bytes)
        .fold(0_u64, u64::saturating_add);
    let root_requested_volume = requested_by_volume.get(&root_volume).copied().unwrap_or(0);
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
        pending_volume_bytes: pending_on(&root_volume),
        requested_volume_bytes: root_requested_volume,
        requested_managed_bytes: root_requested_managed,
        reclaimable_bytes,
    };
    let required = match required_reclaim(budget) {
        Ok(required) => required,
        Err(_) if !candidates.is_empty() => u64::MAX,
        Err(error) => return Err(error),
    };
    let mut pruned_history = false;
    if required > 0 {
        let mut selected = 0_u64;
        let mut selected_ids = HashSet::new();
        for entry in candidates {
            selected = selected.saturating_add(entry.artifact_size_bytes);
            selected_ids.insert(entry.id);
            if required != u64::MAX && selected >= required {
                break;
            }
        }
        let mut index = store.entries.len();
        while index > 0 {
            index -= 1;
            if selected_ids.contains(&store.entries[index].id) {
                let entry = store.entries.remove(index);
                queue_managed_cleanup(&mut store, entry);
            }
        }
        retain_live_delivery_identities(&mut store);
        prepare_cleanup_quarantines(&mut store);
        save_store(&path, &store)?;
        run_saved_cleanup(&path, &mut store)?;
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
        pending_volume_bytes: pending_on(&root_volume),
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
    let requested = product_reservation(product, source_bytes, source_count)?;
    let _guard = ADMISSION_LOCK
        .lock()
        .map_err(|_| "Creation storage admission is unavailable.".to_string())?;
    crate::overlay::creation_source_snapshot::sweep_pressure()?;
    crate::overlay::creation_output::sweep_staging()?;
    crate::overlay::creation_delivery::ensure_cancellation_capacity()?;
    let pending = crate::overlay::creation_delivery::pending_storage_reservations()?;
    ensure_capacity(output_dir, requested, pending)?;
    record()
}

#[cfg(test)]
mod tests;
