//! Persisted record of which MCP integrations the user has installed here. Survives
//! restarts so `connect_all_installed()` can bring them back each session. Same
//! atomic-JSON pattern as `browser/prefs.rs`.

use serde::{Deserialize, Serialize};

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

fn path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_mcp_registry.json")
}

fn load() -> Registry {
    std::fs::File::open(path())
        .ok()
        .and_then(|f| serde_json::from_reader(f).ok())
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn is_installed(id: &str) -> bool {
    load().installed.iter().any(|e| e.id == id)
}

pub(super) fn installed_ids() -> Vec<String> {
    load().installed.into_iter().map(|e| e.id).collect()
}

pub(super) fn mark_installed(id: &str) {
    let mut r = load();
    if !r.installed.iter().any(|e| e.id == id) {
        r.installed.push(Entry {
            id: id.to_string(),
            installed_at: now_secs(),
        });
        let _ = crate::atomic_json::write_json_atomic(&path(), &r);
    }
}

pub(super) fn remove(id: &str) {
    let mut r = load();
    let before = r.installed.len();
    r.installed.retain(|e| e.id != id);
    if r.installed.len() != before {
        let _ = crate::atomic_json::write_json_atomic(&path(), &r);
    }
}
