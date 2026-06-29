//! Per-integration proactive-offer snooze, so an integration the user declined isn't
//! re-nagged (escalating back-off, like `browser/prefs.rs`). Persisted as JSON.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Default)]
struct Prefs {
    declined: HashMap<String, Snooze>,
}

#[derive(Serialize, Deserialize, Default)]
struct Snooze {
    count: u32,
    until: u64, // unix seconds; offers for this id are suppressed until then
}

fn path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_mcp_prefs.json")
}

fn load() -> Prefs {
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

pub(super) fn offer_due(id: &str) -> bool {
    load()
        .declined
        .get(id)
        .is_none_or(|s| now_secs() >= s.until)
}

pub(super) fn record_decline(id: &str) {
    let mut p = load();
    let s = p.declined.entry(id.to_string()).or_default();
    s.count = s.count.saturating_add(1);
    let days: u64 = match s.count {
        1 => 3,
        2 => 14,
        _ => 60,
    };
    s.until = now_secs() + days * 86_400;
    let _ = crate::atomic_json::write_json_atomic(&path(), &p);
}

pub(super) fn clear() {
    let _ = std::fs::remove_file(path());
}
