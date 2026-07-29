//! Crash-safe delivery intents for the public creation mini-apps.
//!
//! An intent contains only the frozen product request and stable delivery IDs.
//! A restarted host can therefore present the same request without inventing a
//! second user action.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

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

static JOURNAL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

pub(crate) struct DeliveryAssignment {
    pub staging_path: PathBuf,
    pub output_path: PathBuf,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    version: u32,
    entries: Vec<Intent>,
}

fn journal_path() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("active-creation-intents.json")
}

pub fn record(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    arguments: Value,
) -> Result<RecordedIntent, String> {
    let _guard = JOURNAL_LOCK
        .lock()
        .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
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
    let _guard = JOURNAL_LOCK
        .lock()
        .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
    let path = journal_path();
    let mut store = read_store(&path)?;
    let before = store.entries.len();
    prune(&mut store, now_ms());
    if store.entries.len() != before {
        write_store(&path, &store)?;
    }
    Ok(store
        .entries
        .into_iter()
        .filter(|intent| intent.product == product)
        .collect())
}

pub(crate) fn load_all() -> Result<Vec<Intent>, String> {
    let _guard = JOURNAL_LOCK
        .lock()
        .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
    let path = journal_path();
    let mut store = read_store(&path)?;
    let before = store.entries.len();
    prune(&mut store, now_ms());
    if store.entries.len() != before {
        write_store(&path, &store)?;
    }
    Ok(store.entries)
}

pub fn clear(product: &str, job_id: &str) {
    let _ = clear_required(product, job_id);
}

pub fn clear_required(product: &str, job_id: &str) -> Result<(), String> {
    let _guard = JOURNAL_LOCK
        .lock()
        .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
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
    write_store(&path, &store)?;
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
    let _guard = JOURNAL_LOCK
        .lock()
        .map_err(|_| "Creation recovery state is unavailable.".to_string())?;
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
    let staging_path = crate::overlay::creation_output::validate_staging_path(
        dispatch_id,
        output_name,
        &staging_dir.join(output_name),
    )?;
    let final_dir = std::fs::canonicalize(final_dir)
        .map_err(|_| "Creation recovery destination is unavailable.".to_string())?;
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

fn record_at(path: &Path, intent: Intent) -> Result<(), String> {
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
        return Err("Creation recovery identity conflicts with its saved request.".to_string());
    }
    if store
        .entries
        .iter()
        .any(|existing| existing.dispatch_id == intent.dispatch_id)
    {
        return Err("Creation recovery identity conflicts with its saved request.".to_string());
    }
    if store.entries.len() >= MAX_ENTRIES {
        return Err("Creation recovery queue is full.".to_string());
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
    write_store(path, &store)
}

fn read_store(path: &Path) -> Result<Store, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
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
    let mut dispatch_ids = std::collections::HashSet::new();
    let mut job_ids = std::collections::HashSet::new();
    if store.version != VERSION
        || store.entries.len() > MAX_ENTRIES
        || store.entries.iter().any(|intent| {
            validate_intent(intent).is_err()
                || !dispatch_ids.insert(intent.dispatch_id.clone())
                || !job_ids.insert((intent.product.clone(), intent.job_id.clone()))
        })
    {
        return Err("Creation recovery state is invalid.".to_string());
    }
    Ok(store)
}

fn prune(store: &mut Store, now: u64) {
    store.entries.retain(|intent| {
        intent.expires_at_ms > now
            && intent.expires_at_ms <= intent.created_at_ms.saturating_add(INTENT_LIFETIME_MS)
            && validate_intent(intent).is_ok()
    });
}

fn validate_intent(intent: &Intent) -> Result<(), String> {
    if !matches!(intent.product.as_str(), "3d" | "svg" | "image")
        || !valid_identifier(&intent.job_id)
        || !valid_identifier(&intent.dispatch_id)
        || intent.expires_at_ms <= intent.created_at_ms
        || intent.expires_at_ms > intent.created_at_ms.saturating_add(INTENT_LIFETIME_MS)
        || intent.accepted_at_ms != intent.created_at_ms
        || intent.deadline_at_ms
            != intent
                .accepted_at_ms
                .saturating_add(crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS)
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

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn write_store(path: &Path, store: &Store) -> Result<(), String> {
    let bytes = serde_json::to_vec(store)
        .map_err(|_| "Creation recovery state could not be saved.".to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("Creation recovery state is too large.".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Creation recovery folder is unavailable.".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Creation recovery folder could not be created: {error}"))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Creation recovery filename is invalid.".to_string())?;
    let temporary = parent.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        replace_file(path, &temporary)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Creation recovery state could not be saved: {error}"))
}

#[cfg(windows)]
fn replace_file(path: &Path, replacement: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return std::fs::rename(replacement, path);
    }
    use std::os::windows::ffi::OsStrExt as _;
    use windows::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};
    use windows::core::PCWSTR;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let replacement = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        ReplaceFileW(
            PCWSTR(path.as_ptr()),
            PCWSTR(replacement.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
    .map_err(std::io::Error::other)
}

#[cfg(not(windows))]
fn replace_file(path: &Path, replacement: &Path) -> std::io::Result<()> {
    std::fs::rename(replacement, path)
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
mod tests {
    use super::*;
    use serde_json::json;

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "sgt-creation-intent-{}-{}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn intent(job: &str, created_at_ms: u64) -> Intent {
        let arguments = json!({"prompt": "A calm landscape", "sourceImagePaths": []});
        Intent {
            product: "image".to_string(),
            job_id: job.to_string(),
            dispatch_id: format!("dispatch-{job}"),
            created_at_ms,
            accepted_at_ms: created_at_ms,
            deadline_at_ms: created_at_ms
                + crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS,
            expires_at_ms: created_at_ms + INTENT_LIFETIME_MS,
            arguments_fingerprint: fingerprint(&arguments).unwrap(),
            arguments,
        }
    }

    #[test]
    fn persisted_intent_keeps_same_delivery_identity_after_reload() {
        let path = test_path();
        record_at(&path, intent("image_1", 10)).unwrap();
        let store = read_store(&path).unwrap();
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].job_id, "image_1");
        assert_eq!(store.entries[0].dispatch_id, "dispatch-image_1");
        clear_at(&path, "image", "image_1", 11).unwrap();
        assert!(read_store(&path).unwrap().entries.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn expired_invalid_and_excess_entries_are_bounded() {
        let path = test_path();
        for index in 0..MAX_ENTRIES {
            record_at(&path, intent(&format!("image_{index}"), 100 + index as u64)).unwrap();
        }
        let before = read_store(&path).unwrap();
        assert_eq!(before.entries.len(), MAX_ENTRIES);
        assert!(record_at(&path, intent("overflow", 1_000)).is_err());
        let mut store = read_store(&path).unwrap();
        assert_eq!(store.entries.len(), MAX_ENTRIES);
        assert_eq!(
            store
                .entries
                .iter()
                .map(|entry| entry.job_id.as_str())
                .collect::<Vec<_>>(),
            before
                .entries
                .iter()
                .map(|entry| entry.job_id.as_str())
                .collect::<Vec<_>>()
        );
        store.entries.push(intent("expired", 1));
        prune(&mut store, INTENT_LIFETIME_MS + 1);
        assert!(!store.entries.iter().any(|entry| entry.job_id == "expired"));
        assert!(
            validate_intent(&Intent {
                dispatch_id: "../invalid".to_string(),
                ..intent("invalid", 10)
            })
            .is_err()
        );
        assert!(validate_persisted_path(Path::new("relative")).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn corrupt_intent_index_is_preserved_and_fails_closed() {
        let path = test_path();
        let corrupt = b"{ invalid-accepted-jobs";
        std::fs::write(&path, corrupt).unwrap();
        assert!(record_at(&path, intent("image_1", 10)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recovery_identity_is_immutable_but_exact_replay_is_idempotent() {
        let path = test_path();
        let saved = intent("image_1", 10);
        record_at(&path, saved.clone()).unwrap();
        let mut replay = saved.clone();
        replay.created_at_ms = 11;
        replay.accepted_at_ms = 11;
        replay.deadline_at_ms = 11 + crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS;
        replay.expires_at_ms = 11 + INTENT_LIFETIME_MS;
        record_at(&path, replay).unwrap();
        assert_eq!(read_store(&path).unwrap().entries.len(), 1);

        let mut changed_dispatch = saved.clone();
        changed_dispatch.dispatch_id = "different-dispatch".to_string();
        assert!(record_at(&path, changed_dispatch).is_err());
        let mut changed_arguments = saved;
        changed_arguments.arguments = json!({"prompt": "A different request"});
        changed_arguments.arguments_fingerprint =
            fingerprint(&changed_arguments.arguments).unwrap();
        assert!(record_at(&path, changed_arguments).is_err());
        assert_eq!(read_store(&path).unwrap().entries.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ambiguous_recovery_identities_fail_closed_without_rewriting() {
        let path = test_path();
        let first = intent("image_1", 10);
        let mut duplicate_dispatch = intent("image_2", 11);
        duplicate_dispatch.dispatch_id = first.dispatch_id.clone();
        let mut duplicate_job = intent("image_1", 12);
        duplicate_job.dispatch_id = "another-dispatch".to_string();
        for ambiguous in [duplicate_dispatch, duplicate_job] {
            write_store(
                &path,
                &Store {
                    version: VERSION,
                    entries: vec![first.clone(), ambiguous],
                },
            )
            .unwrap();
            let before = std::fs::read(&path).unwrap();
            assert!(read_store(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        let _ = std::fs::remove_file(path);
    }
}
