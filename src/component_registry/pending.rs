use std::collections::BTreeSet;
use std::io::Read as _;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::catalog::validate_identifier;

const SCHEMA_VERSION: u32 = 1;
const MAX_PENDING: usize = 256;
const MAX_FILE_BYTES: u64 = 64 * 1024;
static STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingRemovals {
    schema_version: u32,
    component_ids: BTreeSet<String>,
}

pub(super) fn record(id: &str) -> Result<()> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|value| value.into_inner());
    record_at(&path(), id)
}

fn record_at(path: &std::path::Path, id: &str) -> Result<()> {
    validate_identifier(id)?;
    let mut state = read_at(path)?;
    state.component_ids.insert(id.to_string());
    write_at(path, &state)
}

pub(super) fn clear(id: &str) -> Result<()> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|value| value.into_inner());
    clear_at(&path(), id)
}

fn clear_at(path: &std::path::Path, id: &str) -> Result<()> {
    validate_identifier(id)?;
    let mut state = read_at(path)?;
    state.component_ids.remove(id);
    write_at(path, &state)
}

pub(super) fn list() -> Result<Vec<String>> {
    Ok(snapshot()?.0)
}

pub(super) fn snapshot() -> Result<(Vec<String>, Vec<String>)> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|value| value.into_inner());
    Ok(partition(read_unlocked()?, |id| {
        super::embedded_catalog().is_removable(id)
    }))
}

fn read_unlocked() -> Result<PendingRemovals> {
    read_at(&path())
}

fn read_at(path: &std::path::Path) -> Result<PendingRemovals> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
        options
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PendingRemovals {
                schema_version: SCHEMA_VERSION,
                component_ids: BTreeSet::new(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || super::receipt::is_reparse_point(&metadata)
        || metadata.len() > MAX_FILE_BYTES
    {
        bail!("pending component-removal state is unsafe");
    }
    let mut body = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut body)?;
    if body.len() as u64 > MAX_FILE_BYTES {
        bail!("pending component-removal state is too large");
    }
    let state: PendingRemovals = serde_json::from_slice(&body)?;
    validate(&state)?;
    Ok(state)
}

fn write_at(path: &std::path::Path, state: &PendingRemovals) -> Result<()> {
    validate(state)?;
    if serde_json::to_vec_pretty(state)?.len() as u64 > MAX_FILE_BYTES {
        bail!("pending component-removal state is too large");
    }
    if state.component_ids.is_empty() {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::atomic_json::write_json_atomic(path, state)?;
    Ok(())
}

#[cfg(test)]
#[path = "pending_store_tests.rs"]
mod tests;

fn validate(state: &PendingRemovals) -> Result<()> {
    if state.schema_version != SCHEMA_VERSION || state.component_ids.len() > MAX_PENDING {
        bail!("pending component-removal state is invalid");
    }
    for id in &state.component_ids {
        validate_identifier(id)?;
    }
    Ok(())
}

fn partition(
    state: PendingRemovals,
    is_removable: impl Fn(&str) -> bool,
) -> (Vec<String>, Vec<String>) {
    state
        .component_ids
        .into_iter()
        .partition(|id| is_removable(id))
}

fn path() -> std::path::PathBuf {
    #[cfg(test)]
    return std::env::temp_dir().join(format!(
        "screen-goated-toolbox-component-removals-{}.json",
        std::process::id()
    ));
    #[cfg(not(test))]
    crate::paths::app_runtime_local_data_dir().join("pending-component-removals.json")
}
