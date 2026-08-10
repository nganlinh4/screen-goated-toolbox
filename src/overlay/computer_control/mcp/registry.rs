//! Persisted record of which MCP integrations the user has installed here. Survives
//! restarts so `connect_all_installed()` can bring them back each session. Same
//! atomic-JSON pattern as `browser/prefs.rs`.

use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    installed: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    id: String,
    #[serde(default)]
    installed_at: u64,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(load_from_disk()));

fn persisted_path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_mcp_registry.json")
}

fn writable_path() -> std::path::PathBuf {
    crate::paths::app_runtime_config_dir().join("cc_mcp_registry.json")
}

fn load_from_disk() -> Registry {
    [writable_path(), persisted_path()]
        .into_iter()
        .find_map(|path| {
            std::fs::File::open(path)
                .ok()
                .and_then(|file| serde_json::from_reader(file).ok())
        })
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn is_installed(id: &str) -> bool {
    REGISTRY
        .lock()
        .map(|registry| registry.installed.iter().any(|entry| entry.id == id))
        .unwrap_or(false)
}

pub(super) fn installed_ids() -> Vec<String> {
    REGISTRY
        .lock()
        .map(|registry| {
            registry
                .installed
                .iter()
                .map(|entry| entry.id.clone())
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn mark_installed(id: &str) {
    let Ok(mut registry) = REGISTRY.lock() else {
        return;
    };
    if !registry.installed.iter().any(|entry| entry.id == id) {
        registry.installed.push(Entry {
            id: id.to_string(),
            installed_at: now_secs(),
        });
        if let Err(error) = crate::atomic_json::write_json_atomic(&writable_path(), &*registry) {
            registry.installed.retain(|entry| entry.id != id);
            crate::log_info!("[MCP] Could not persist installed integration: {error:#}");
        }
    }
}

pub(super) fn remove(id: &str) {
    if let Err(error) = try_remove(id) {
        crate::log_info!("[MCP] Could not persist removed integration: {error:#}");
    }
}

pub(super) fn try_remove(id: &str) -> anyhow::Result<()> {
    let mut registry = REGISTRY
        .lock()
        .map_err(|_| anyhow::anyhow!("MCP integration registry is unavailable"))?;
    let before = registry.installed.len();
    registry.installed.retain(|entry| entry.id != id);
    if registry.installed.len() == before {
        return Ok(());
    }
    if let Err(error) = crate::atomic_json::write_json_atomic(&writable_path(), &*registry) {
        *registry = load_from_disk();
        return Err(error.into());
    }
    Ok(())
}
