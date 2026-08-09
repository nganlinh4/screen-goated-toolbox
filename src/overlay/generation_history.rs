use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod admission;
mod deletion;
mod delivery;
mod rename;
mod retention;
mod source_paths;
pub(crate) use admission::admit_and_record;
use deletion::{delete_all_at, delete_at};
pub(crate) use delivery::*;
use rename::*;
use retention::*;
pub(crate) use source_paths::live_source_paths;

const DEFAULT_RESULTS_PER_TOOL: usize = 50;
const MAX_MANAGED_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_HISTORY_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_HISTORY_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HISTORY_METADATA_BYTES: usize = 512 * 1024;
const MAX_HISTORY_PATH_BYTES: usize = 8 * 1024;
const MAX_DELIVERY_ID_BYTES: usize = 512;
pub(crate) const THREE_D_RESULT_RESERVATION_BYTES: u64 = 100 * 1024 * 1024;
pub(crate) const SVG_RESULT_RESERVATION_BYTES: u64 = 12 * 1024 * 1024;
pub(crate) const IMAGE_RESULT_RESERVATION_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const IMAGE_REFERENCE_RESERVATION_BYTES: u64 = 100 * 1024 * 1024;
pub(crate) const SOURCE_PRESENTATION_RESERVATION_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultHistoryEntry {
    pub id: String,
    pub tool: String,
    pub source_path: String,
    pub output_path: String,
    pub output_name: String,
    pub created_at_ms: u64,
    #[serde(default)]
    pub artifact_size_bytes: u64,
    #[serde(default)]
    pub artifact_sha256: String,
    #[serde(default)]
    pub managed_artifact: bool,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultHistoryView {
    pub id: String,
    pub tool: String,
    pub source_path: String,
    pub output_path: String,
    pub output_name: String,
    pub created_at_ms: u64,
    pub metadata: Value,
}

impl From<&ResultHistoryEntry> for ResultHistoryView {
    fn from(entry: &ResultHistoryEntry) -> Self {
        Self {
            id: entry.id.clone(),
            tool: entry.tool.clone(),
            source_path: entry.source_path.clone(),
            output_path: entry.output_path.clone(),
            output_name: entry.output_name.clone(),
            created_at_ms: entry.created_at_ms,
            metadata: entry.metadata.clone(),
        }
    }
}

pub fn public_entries(entries: &[ResultHistoryEntry]) -> Vec<ResultHistoryView> {
    entries.iter().map(ResultHistoryView::from).collect()
}

pub fn public_entry(entry: &ResultHistoryEntry) -> ResultHistoryView {
    ResultHistoryView::from(entry)
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingCleanup {
    output_path: String,
    artifact_size_bytes: u64,
    artifact_sha256: String,
    #[serde(default)]
    artifact_file_identity: String,
    #[serde(default)]
    quarantine_path: String,
    #[serde(default)]
    history_entry: Option<ResultHistoryEntry>,
    #[serde(default)]
    resolution: CleanupResolution,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum CleanupResolution {
    #[default]
    Pending,
    Restore {
        target_path: String,
    },
    Relinquish {
        target_path: String,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDeliveryIdentity {
    tool: String,
    dispatch_id: String,
    entry_id: String,
    #[serde(default)]
    artifact_file_identity: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingRename {
    tool: String,
    entry_id: String,
    previous_path: String,
    next_path: String,
    next_name: String,
    #[serde(default)]
    artifact_size_bytes: u64,
    #[serde(default)]
    artifact_sha256: String,
    #[serde(default)]
    artifact_file_identity: String,
}

#[derive(Default, Deserialize, Serialize)]
struct ResultHistoryStore {
    entries: Vec<ResultHistoryEntry>,
    #[serde(default)]
    pending_cleanup: Vec<PendingCleanup>,
    #[serde(default)]
    delivery_identities: Vec<HistoryDeliveryIdentity>,
    #[serde(default)]
    pending_renames: Vec<PendingRename>,
}

static HISTORY_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static HISTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static HISTORY_MAINTENANCE_RUNNING: AtomicBool = AtomicBool::new(false);
static REQUESTED_RESULTS_PER_TOOL: AtomicUsize = AtomicUsize::new(usize::MAX);

fn history_path() -> PathBuf {
    crate::paths::app_local_data_dir().join("creation-result-history.json")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn validate_tool(tool: &str) -> Result<(), String> {
    if matches!(tool, "3d" | "svg" | "image") {
        Ok(())
    } else {
        Err("Unknown result history tool.".to_string())
    }
}

fn results_per_tool_limit() -> usize {
    crate::APP
        .lock()
        .map(|app| app.config.max_history_items.clamp(10, 200))
        .unwrap_or(DEFAULT_RESULTS_PER_TOOL)
}

fn load_store(path: &Path) -> Result<ResultHistoryStore, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ResultHistoryStore::default());
        }
        Err(_) => return Err("Result history is unavailable.".to_string()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_HISTORY_INDEX_BYTES {
        return Err("Saved result history is invalid.".to_string());
    }
    serde_json::from_reader(file.take(MAX_HISTORY_INDEX_BYTES + 1))
        .map_err(|_| "Saved result history is invalid.".to_string())
}

fn save_store(path: &Path, store: &ResultHistoryStore) -> Result<(), String> {
    let serialized = serde_json::to_vec_pretty(store)
        .map_err(|_| "Could not serialize result history.".to_string())?;
    if serialized.len() as u64 > MAX_HISTORY_INDEX_BYTES {
        return Err("Result history exceeds its storage limit.".to_string());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create result history folder: {error}"))?;
    }
    crate::atomic_json::write_json_atomic(path, store)
        .map_err(|error| format!("Could not save result history: {error}"))
}

fn same_path(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn inspect_recorded_artifact(path: &Path) -> Result<(u64, String, bool), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect artifact: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_HISTORY_ARTIFACT_BYTES {
        return Err("Artifact is not a supported regular file.".to_string());
    }
    let managed = is_managed_artifact(path);
    if !managed {
        return Ok((metadata.len(), String::new(), false));
    }
    let (size, digest) = digest_file(path)?;
    Ok((size, digest, true))
}

#[derive(Clone, Copy)]
struct DeliveryIdentity<'a> {
    dispatch_id: &'a str,
    artifact_file_identity: &'a str,
}

struct RecordOptions<'a> {
    results_per_tool: usize,
    inspected_artifact: Option<(u64, String, bool)>,
    delivery: Option<DeliveryIdentity<'a>>,
    protected_paths: &'a std::collections::HashSet<String>,
}

#[cfg(test)]
fn list_at(
    path: &Path,
    tool: &str,
    results_per_tool: usize,
) -> Result<Vec<ResultHistoryEntry>, String> {
    list_at_protected(
        path,
        tool,
        results_per_tool,
        &std::collections::HashSet::new(),
    )
}

fn list_at_protected(
    path: &Path,
    tool: &str,
    results_per_tool: usize,
    protected_paths: &std::collections::HashSet<String>,
) -> Result<Vec<ResultHistoryEntry>, String> {
    validate_tool(tool)?;
    let mut store = load_store(path)?;
    reconcile_store_protected(path, &mut store, results_per_tool, protected_paths)?;
    let mut entries = store
        .entries
        .into_iter()
        .filter(|entry| entry.tool == tool)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at_ms));
    Ok(entries)
}

fn list_snapshot_at(path: &Path, tool: &str) -> Result<Vec<ResultHistoryEntry>, String> {
    validate_tool(tool)?;
    let mut entries = load_store(path)?
        .entries
        .into_iter()
        .filter(|entry| {
            entry.tool == tool
                && std::fs::symlink_metadata(&entry.output_path)
                    .map(|metadata| metadata.file_type().is_file())
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at_ms));
    Ok(entries)
}

#[cfg(test)]
fn record_at(
    path: &Path,
    tool: &str,
    source_path: &str,
    output_path: &str,
    metadata: Value,
    results_per_tool: usize,
    evidence: (Option<(u64, String, bool)>, Option<DeliveryIdentity<'_>>),
) -> Result<ResultHistoryEntry, String> {
    let (inspected_artifact, delivery) = evidence;
    record_at_protected(
        path,
        tool,
        source_path,
        output_path,
        metadata,
        RecordOptions {
            results_per_tool,
            inspected_artifact,
            delivery,
            protected_paths: &std::collections::HashSet::new(),
        },
    )
}

fn record_at_protected(
    path: &Path,
    tool: &str,
    source_path: &str,
    output_path: &str,
    metadata: Value,
    options: RecordOptions<'_>,
) -> Result<ResultHistoryEntry, String> {
    let RecordOptions {
        results_per_tool,
        inspected_artifact,
        delivery,
        protected_paths,
    } = options;
    validate_tool(tool)?;
    let output = PathBuf::from(output_path);
    let output_metadata = std::fs::symlink_metadata(&output)
        .map_err(|_| format!("Result file does not exist: {}", output.display()))?;
    if !output_metadata.file_type().is_file() {
        return Err(format!("Result file does not exist: {}", output.display()));
    }
    let output_name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| "Result filename is missing.".to_string())?;
    let output_path = output.to_string_lossy().to_string();
    if source_path.len() > MAX_HISTORY_PATH_BYTES
        || output_path.len() > MAX_HISTORY_PATH_BYTES
        || output_name.len() > 1_024
        || delivery.is_some_and(|identity| {
            identity.dispatch_id.is_empty()
                || identity.dispatch_id.len() > MAX_DELIVERY_ID_BYTES
                || identity.dispatch_id.chars().any(char::is_control)
                || !crate::overlay::creation_file_identity::valid(identity.artifact_file_identity)
        })
        || serde_json::to_vec(&metadata)
            .map(|bytes| bytes.len() > MAX_HISTORY_METADATA_BYTES)
            .unwrap_or(true)
    {
        return Err("Result history entry exceeds its storage limit.".to_string());
    }
    let mut store = load_store(path)?;
    reconcile_store_protected(path, &mut store, results_per_tool, protected_paths)?;
    let (artifact_size_bytes, artifact_sha256, managed_artifact) = match inspected_artifact {
        Some(details) => details,
        None => inspect_recorded_artifact(&output)?,
    };
    if output_metadata.len() != artifact_size_bytes {
        return Err("Result file changed before history could record it.".to_string());
    }
    if let Some(identity) = delivery
        && let Some(saved) = store
            .delivery_identities
            .iter()
            .find(|saved| saved.tool == tool && saved.dispatch_id == identity.dispatch_id)
    {
        let entry = store
            .entries
            .iter()
            .find(|entry| entry.id == saved.entry_id)
            .ok_or_else(|| "Saved delivery history is incomplete.".to_string())?;
        if !same_path(&entry.output_path, &output_path)
            || entry.source_path != source_path
            || entry.metadata != metadata
            || entry.artifact_size_bytes != artifact_size_bytes
            || entry.artifact_sha256 != artifact_sha256
            || saved.artifact_file_identity != identity.artifact_file_identity
        {
            return Err("Saved delivery history conflicts with this result.".to_string());
        }
        return Ok(entry.clone());
    }
    let timestamp = now_ms();
    let existing = delivery.is_none().then(|| {
        store
            .entries
            .iter_mut()
            .find(|entry| entry.tool == tool && same_path(&entry.output_path, &output_path))
    });
    let entry = if let Some(existing) = existing.flatten() {
        existing.source_path = source_path.to_string();
        existing.output_path = output_path;
        existing.output_name = output_name;
        existing.created_at_ms = timestamp;
        existing.artifact_size_bytes = artifact_size_bytes;
        existing.artifact_sha256 = artifact_sha256;
        existing.managed_artifact = managed_artifact;
        existing.metadata = metadata;
        existing.clone()
    } else {
        let entry = ResultHistoryEntry {
            id: format!(
                "{tool}_{timestamp}_{}",
                HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            tool: tool.to_string(),
            source_path: source_path.to_string(),
            output_path,
            output_name,
            created_at_ms: timestamp,
            artifact_size_bytes,
            artifact_sha256,
            managed_artifact,
            metadata,
        };
        store.entries.push(entry.clone());
        entry
    };
    if let Some(identity) = delivery {
        store.delivery_identities.push(HistoryDeliveryIdentity {
            tool: tool.to_string(),
            dispatch_id: identity.dispatch_id.to_string(),
            entry_id: entry.id.clone(),
            artifact_file_identity: identity.artifact_file_identity.to_string(),
        });
    }
    prune_store_protected(&mut store, results_per_tool, protected_paths);
    retain_live_delivery_identities(&mut store);
    prepare_cleanup_quarantines(&mut store);
    save_store(path, &store)?;
    Ok(entry)
}

fn retain_live_delivery_identities(store: &mut ResultHistoryStore) -> bool {
    let live = store
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let previous = store.delivery_identities.len();
    store
        .delivery_identities
        .retain(|identity| live.contains(identity.entry_id.as_str()));
    store.delivery_identities.len() != previous
}

fn maintain_at(path: &Path) -> Result<(), String> {
    let mut store = load_store(path)?;
    if prepare_cleanup_quarantines(&mut store) {
        // The exact original/quarantine mapping must be durable before a move.
        save_store(path, &store)?;
    }
    if run_pending_cleanup_at(path, &mut store)? {
        save_store(path, &store)?;
    }
    Ok(())
}

fn schedule_maintenance(path: PathBuf) {
    if HISTORY_MAINTENANCE_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    std::thread::spawn(move || {
        loop {
            let requested = REQUESTED_RESULTS_PER_TOOL.swap(usize::MAX, Ordering::AcqRel);
            let protected = if requested == usize::MAX {
                Ok(std::collections::HashSet::new())
            } else {
                crate::overlay::creation_delivery::pending_output_paths()
                    .map(|paths| paths.into_iter().map(|path| path_identity(&path)).collect())
            };
            let result = protected.and_then(|protected| {
                HISTORY_LOCK
                    .lock()
                    .map_err(|_| "Result history is unavailable.".to_string())
                    .and_then(|_guard| {
                        if requested != usize::MAX {
                            let mut store = load_store(&path)?;
                            reconcile_store_protected(
                                &path,
                                &mut store,
                                requested.clamp(10, 200),
                                &protected,
                            )?;
                        }
                        maintain_at(&path)
                    })
            });
            if let Err(error) = result {
                crate::log_info!("[Creation history] Deferred cleanup could not finish: {error}");
            }
            if REQUESTED_RESULTS_PER_TOOL.load(Ordering::Acquire) == usize::MAX {
                break;
            }
        }
        HISTORY_MAINTENANCE_RUNNING.store(false, Ordering::Release);
        if REQUESTED_RESULTS_PER_TOOL.load(Ordering::Acquire) != usize::MAX {
            schedule_maintenance(path);
        }
    });
}

pub fn list(tool: &str) -> Result<Vec<ResultHistoryEntry>, String> {
    crate::overlay::creation_delivery::schedule_reconciliation(tool);
    let protected_paths = crate::overlay::creation_delivery::pending_output_paths()?
        .into_iter()
        .map(|path| path_identity(&path))
        .collect();
    // Read app configuration before taking the history lock. Some UI paths hold
    // the app lock while closing mini apps, so the reverse order could deadlock.
    let results_per_tool = results_per_tool_limit();
    let path = history_path();
    let entries = match HISTORY_LOCK.try_lock() {
        Ok(_guard) => list_at_protected(&path, tool, results_per_tool, &protected_paths)?,
        Err(TryLockError::WouldBlock) => list_snapshot_at(&path, tool)?,
        Err(TryLockError::Poisoned(_)) => {
            return Err("Result history is unavailable.".to_string());
        }
    };
    schedule_maintenance(path);
    Ok(entries)
}

pub fn request_prune(results_per_tool: usize) {
    REQUESTED_RESULTS_PER_TOOL.store(results_per_tool.clamp(10, 200), Ordering::Release);
    schedule_maintenance(history_path());
}

pub fn rename(tool: &str, id: &str, new_name: &str) -> Result<ResultHistoryEntry, String> {
    let results_per_tool = results_per_tool_limit();
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    rename_at(&history_path(), tool, id, new_name, results_per_tool)
}

pub fn delete(tool: &str, id: &str) -> Result<(), String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    delete_at(&history_path(), tool, id)
}

pub fn delete_all(tool: &str) -> Result<usize, String> {
    let _guard = HISTORY_LOCK
        .lock()
        .map_err(|_| "Result history is unavailable.".to_string())?;
    delete_all_at(&history_path(), tool)
}

#[cfg(test)]
mod tests;
