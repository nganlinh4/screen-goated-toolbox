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
    validate_identifier(id)?;
    let _guard = STORE_LOCK.lock().unwrap_or_else(|value| value.into_inner());
    let mut state = read_unlocked()?;
    state.component_ids.insert(id.to_string());
    write_unlocked(&state)
}

pub(super) fn clear(id: &str) -> Result<()> {
    validate_identifier(id)?;
    let _guard = STORE_LOCK.lock().unwrap_or_else(|value| value.into_inner());
    let mut state = read_unlocked()?;
    state.component_ids.remove(id);
    write_unlocked(&state)
}

pub(super) fn list() -> Result<Vec<String>> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|value| value.into_inner());
    Ok(read_unlocked()?.component_ids.into_iter().collect())
}

fn read_unlocked() -> Result<PendingRemovals> {
    let path = path();
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return Ok(PendingRemovals {
            schema_version: SCHEMA_VERSION,
            component_ids: BTreeSet::new(),
        });
    };
    if !metadata.is_file()
        || super::receipt::is_reparse_point(&metadata)
        || metadata.len() > MAX_FILE_BYTES
    {
        bail!("pending component-removal state is unsafe");
    }
    let mut body = String::new();
    std::fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut body)?;
    let state: PendingRemovals = serde_json::from_str(&body)?;
    validate(&state)?;
    Ok(state)
}

fn write_unlocked(state: &PendingRemovals) -> Result<()> {
    validate(state)?;
    let path = path();
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
    crate::atomic_json::write_json_atomic(&path, state)?;
    Ok(())
}

fn validate(state: &PendingRemovals) -> Result<()> {
    if state.schema_version != SCHEMA_VERSION || state.component_ids.len() > MAX_PENDING {
        bail!("pending component-removal state is invalid");
    }
    for id in &state.component_ids {
        validate_identifier(id)?;
        if !super::embedded_catalog().is_removable(id) {
            bail!("pending component-removal state names an unknown component");
        }
    }
    Ok(())
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
