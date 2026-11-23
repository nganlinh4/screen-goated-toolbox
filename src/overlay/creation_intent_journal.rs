//! Crash-safe delivery intents for the public creation mini-apps.
//!
//! An intent contains only the frozen product request and stable delivery IDs.
//! A restarted host can therefore present the same request without inventing a
//! second user action.

use crate::overlay::store_support::{now_ms, valid_identifier};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

const VERSION: u32 = 1;
// Three independent creation tools may each retain up to 50 accepted jobs.
// Keep headroom for an in-flight continuation without ever evicting an
// unexpired accepted intent.
const MAX_ENTRIES: usize = 192;
const MAX_FILE_BYTES: u64 = 24 * 1024 * 1024;
const MAX_ARGUMENT_BYTES: usize = 512 * 1024;
const MAX_PERSISTED_PATH_BYTES: usize = 8 * 1024;
const INTENT_LIFETIME_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
mod locking;
mod storage;
mod transaction;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub product: String,
    pub job_id: String,
    pub dispatch_id: String,
    pub created_at_ms: u64,
    #[serde(default)]
    pub accepted_at_ms: u64,
    #[serde(default)]
    pub deadline_at_ms: u64,
    pub expires_at_ms: u64,
    pub arguments: Value,
    pub arguments_fingerprint: String,
}

pub(crate) struct RecordedIntent {
    pub arguments_fingerprint: String,
    pub deadline_at_ms: u64,
}

#[derive(Debug)]
pub(crate) struct RecordError {
    message: String,
    uncertain: bool,
}

impl RecordError {
    pub(crate) fn is_uncertain(&self) -> bool {
        self.uncertain
    }
    fn uncertain(message: String) -> Self {
        Self {
            message: format!(
                "Creation request status is uncertain; do not retry until recovery is checked. {message}"
            ),
            uncertain: true,
        }
    }
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(formatter)
    }
}

impl From<String> for RecordError {
    fn from(message: String) -> Self {
        Self {
            message,
            uncertain: false,
        }
    }
}

impl From<&str> for RecordError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

pub(crate) struct DeliveryAssignment {
    pub staging_path: PathBuf,
    pub output_path: PathBuf,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    version: u32,
    entries: Vec<Intent>,
    /// Set when reading dropped individually invalid entries, so the next
    /// write persists the repaired store.
    #[serde(skip)]
    dropped_invalid: bool,
}

fn journal_path() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("active-creation-intents.json")
}

pub(crate) fn record(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    arguments: Value,
) -> Result<RecordedIntent, RecordError> {
    let _guard = locking::acquire()?;
    let created_at_ms = now_ms();
    let deadline_at_ms =
        created_at_ms.saturating_add(crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS);
    let arguments_fingerprint = fingerprint(&arguments)?;
    record_at(
        &journal_path(),
        Intent {
            product: product.to_string(),
            job_id: job_id.to_string(),
            dispatch_id: dispatch_id.to_string(),
            created_at_ms,
            accepted_at_ms: created_at_ms,
            deadline_at_ms,
            expires_at_ms: created_at_ms.saturating_add(INTENT_LIFETIME_MS),
            arguments,
            arguments_fingerprint: arguments_fingerprint.clone(),
        },
    )?;
    Ok(RecordedIntent {
        arguments_fingerprint,
        deadline_at_ms,
    })
}

pub fn load(product: &str) -> Result<Vec<Intent>, String> {
    let _guard = locking::acquire()?;
    let path = journal_path();
    let mut store = read_store(&path)?;
    let before = store.entries.len();
    prune(&mut store, now_ms());
    if store.dropped_invalid || store.entries.len() != before {
        write_store(&path, &store).map_err(|error| error.to_string())?;
    }
    Ok(store
        .entries
        .into_iter()
        .filter(|intent| intent.product == product)
        .collect())
}

pub(crate) fn load_all() -> Result<Vec<Intent>, String> {
    let _guard = locking::acquire()?;
    let path = journal_path();
    let mut store = read_store(&path)?;
    let before = store.entries.len();
    prune(&mut store, now_ms());
    if store.dropped_invalid || store.entries.len() != before {
        write_store(&path, &store).map_err(|error| error.to_string())?;
    }
    Ok(store.entries)
}

pub fn clear(product: &str, job_id: &str) {
    let _ = clear_required(product, job_id);
}

pub fn clear_required(product: &str, job_id: &str) -> Result<(), String> {
    let _guard = locking::acquire()?;
    let path = journal_path();
    let mut store = read_store(&path)?;
    prune(&mut store, now_ms());
    let cleared = store
        .entries
        .iter()
        .filter(|intent| intent.product == product && intent.job_id == job_id)
        .cloned()
        .collect::<Vec<_>>();
    store
        .entries
        .retain(|intent| intent.product != product || intent.job_id != job_id);
    store.version = VERSION;
    write_store(&path, &store).map_err(|error| error.to_string())?;
    for intent in &cleared {
        crate::overlay::creation_source_snapshot::release_for_cleared_intent(intent);
    }
    Ok(())
}

pub fn verify_arguments(intent: &Intent, arguments: &Value) -> Result<(), String> {
    if fingerprint(arguments)? != intent.arguments_fingerprint {
        return Err("Creation recovery request changed unexpectedly.".to_string());
    }
    Ok(())
}

pub(crate) fn verify_delivery_assignment(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    request_fingerprint: &str,
    output_name: &str,
) -> Result<DeliveryAssignment, String> {
    let assignment = saved_delivery_assignment(
        product,
        job_id,
        dispatch_id,
        request_fingerprint,
        output_name,
    )?;
    let staging_path = crate::overlay::creation_output::validate_staging_path(
        dispatch_id,
        output_name,
        &assignment.staging_path,
    )?;
    let parent = assignment
        .output_path
        .parent()
        .ok_or("Creation recovery destination is invalid.")?;
    let final_dir = std::fs::canonicalize(parent)
        .map_err(|_| "Creation recovery destination is unavailable.".to_string())?;
    Ok(DeliveryAssignment {
        staging_path,
        output_path: crate::overlay::creation_output::assigned_path(&final_dir, output_name)?,
    })
}

pub(crate) fn saved_delivery_assignment(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    request_fingerprint: &str,
    output_name: &str,
) -> Result<DeliveryAssignment, String> {
    let _guard = locking::acquire()?;
    let store = read_store(&journal_path())?;
    let intent = store
        .entries
        .iter()
        .find(|intent| intent.product == product && intent.job_id == job_id)
        .ok_or_else(|| "Creation recovery assignment is missing.".to_string())?;
    if intent.dispatch_id != dispatch_id || intent.arguments_fingerprint != request_fingerprint {
        return Err("Creation recovery assignment conflicts with its delivery.".to_string());
    }
    let string = |key: &str| {
        intent
            .arguments
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| "Creation recovery assignment is invalid.".to_string())
    };
    if intent.arguments.get("outputName").and_then(Value::as_str) != Some(output_name) {
        return Err("Creation recovery assignment conflicts with its delivery.".to_string());
    }
    let staging_dir = string("outputDir")?;
    let final_dir = string("finalOutputDir")?;
    validate_persisted_path(&staging_dir)?;
    validate_persisted_path(&final_dir)?;
    let staging_path = crate::overlay::creation_output::assigned_path(&staging_dir, output_name)?;
    crate::overlay::creation_output::validate_staging_assignment(
        dispatch_id,
        output_name,
        &staging_path,
    )?;
    crate::overlay::creation_path::normalized_absolute(&final_dir)?;
    let output_path = crate::overlay::creation_output::assigned_path(&final_dir, output_name)?;
    Ok(DeliveryAssignment {
        staging_path,
        output_path,
    })
}

pub fn validate_persisted_path(path: &Path) -> Result<(), String> {
    let value = path.to_string_lossy();
    if !path.is_absolute()
        || value.is_empty()
        || value.len() > MAX_PERSISTED_PATH_BYTES
        || value.contains('\0')
    {
        return Err("Creation recovery path is invalid.".to_string());
    }
    Ok(())
}

fn record_at(path: &Path, intent: Intent) -> Result<(), RecordError> {
    validate_intent(&intent)?;
    let mut store = read_store(path)?;
    prune(&mut store, intent.created_at_ms);
    if let Some(existing) = store
        .entries
        .iter()
        .find(|existing| existing.product == intent.product && existing.job_id == intent.job_id)
    {
        if existing.dispatch_id == intent.dispatch_id
            && existing.arguments_fingerprint == intent.arguments_fingerprint
            && existing.arguments == intent.arguments
        {
            return Ok(());
        }
        return Err(
            "Creation recovery identity conflicts with its saved request."
                .to_string()
                .into(),
        );
    }
    if store
        .entries
        .iter()
        .any(|existing| existing.dispatch_id == intent.dispatch_id)
    {
        return Err(
            "Creation recovery identity conflicts with its saved request."
                .to_string()
                .into(),
        );
    }
    if store.entries.len() >= MAX_ENTRIES {
        return Err("Creation recovery queue is full.".to_string().into());
    }
    store.entries.push(intent);
    store.version = VERSION;
    write_store(path, &store)
}

#[cfg(test)]
fn clear_at(path: &Path, product: &str, job_id: &str, now: u64) -> Result<(), String> {
    let mut store = read_store(path)?;
    prune(&mut store, now);
    store
        .entries
        .retain(|intent| intent.product != product || intent.job_id != job_id);
    store.version = VERSION;
    write_store(path, &store).map_err(|error| error.to_string())
}

fn read_store(path: &Path) -> Result<Store, String> {
    transaction::reconcile(path)?;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if storage::has_recovery_candidate(path)? {
                return Err("Creation recovery state requires reconciliation.".to_string());
            }
            return Ok(Store::default());
        }
        Err(_) => return Err("Creation recovery state is unavailable.".to_string()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err("Creation recovery state is invalid.".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    if file
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_FILE_BYTES
    {
        return Err("Creation recovery state is invalid.".to_string());
    }
    let mut store: Store = serde_json::from_slice(&bytes)
        .map_err(|_| "Creation recovery state is invalid.".to_string())?;
    for intent in &mut store.entries {
        if intent.accepted_at_ms == 0 && intent.deadline_at_ms == 0 {
            intent.accepted_at_ms = intent.created_at_ms;
            intent.deadline_at_ms = intent
                .created_at_ms
                .saturating_add(crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS);
        }
    }
    if store.version != VERSION || store.entries.len() > MAX_ENTRIES {
        return Err("Creation recovery state is invalid.".to_string());
    }
    // One damaged entry must not disable every creation tool: drop it alone.
    let before = store.entries.len();
    store
        .entries
        .retain(|intent| validate_intent(intent).is_ok());
    let dropped = before - store.entries.len();
    if dropped > 0 {
        crate::log_info!("[CreationIntent] dropped {dropped} invalid recovery entries");
        store.dropped_invalid = true;
    }
    // Ambiguous identities stay fail-closed: recovering either copy could
    // deliver the wrong request.
    let mut dispatch_ids = std::collections::HashSet::new();
    let mut job_ids = std::collections::HashSet::new();
    if store.entries.iter().any(|intent| {
        !dispatch_ids.insert(intent.dispatch_id.clone())
            || !job_ids.insert((intent.product.clone(), intent.job_id.clone()))
    }) {
        return Err("Creation recovery state is invalid.".to_string());
    }
    Ok(store)
}

/// Products whose recovery turns a request past its deadline into a visible
/// failed job before clearing its intent. Every intent expires after its
/// deadline, so age alone must never remove one of theirs: the user would
/// never learn how the accepted request ended.
fn resolved_by_owner(product: &str) -> bool {
    matches!(product, "3d" | "svg")
}

fn prune(store: &mut Store, now: u64) {
    store.entries.retain(|intent| {
        (intent.expires_at_ms > now || resolved_by_owner(&intent.product))
            && intent.expires_at_ms <= intent.created_at_ms.saturating_add(INTENT_LIFETIME_MS)
    });
}

fn validate_intent(intent: &Intent) -> Result<(), String> {
    if !matches!(intent.product.as_str(), "3d" | "svg" | "image")
        || !valid_identifier(&intent.job_id)
        || !valid_identifier(&intent.dispatch_id)
        || intent.expires_at_ms <= intent.created_at_ms
        || intent.expires_at_ms > intent.created_at_ms.saturating_add(INTENT_LIFETIME_MS)
        || intent.accepted_at_ms != intent.created_at_ms
        // The deadline is persisted data: a release may change the wall-time
        // limit, and the supervisor caps the remaining time anyway.
        || intent.deadline_at_ms <= intent.accepted_at_ms
        || intent.deadline_at_ms > intent.accepted_at_ms.saturating_add(INTENT_LIFETIME_MS)
    {
        return Err("Creation recovery entry is invalid.".to_string());
    }
    let argument_bytes = serde_json::to_vec(&intent.arguments)
        .map_err(|_| "Creation recovery entry is invalid.".to_string())?;
    if argument_bytes.len() > MAX_ARGUMENT_BYTES
        || !intent.arguments.is_object()
        || fingerprint(&intent.arguments)? != intent.arguments_fingerprint
    {
        return Err("Creation recovery entry is too large.".to_string());
    }
    Ok(())
}

fn fingerprint(arguments: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(arguments)
        .map_err(|_| "Creation recovery request is invalid.".to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn write_store(path: &Path, store: &Store) -> Result<(), RecordError> {
    let bytes = serde_json::to_vec(store)
        .map_err(|_| "Creation recovery state could not be saved.".to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("Creation recovery state is too large.".to_string().into());
    }
    storage::write(path, &bytes)
}

#[cfg(test)]
mod tests;
