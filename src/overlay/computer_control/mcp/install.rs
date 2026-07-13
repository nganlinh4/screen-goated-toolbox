use super::{registry, runtime};
use std::collections::HashSet;

/// Ids whose install thread is in flight (so the UI shows "installing…" and won't
/// double-spawn).
pub(super) fn installing() -> &'static parking_lot::Mutex<HashSet<String>> {
    static INSTALLING: std::sync::OnceLock<parking_lot::Mutex<HashSet<String>>> =
        std::sync::OnceLock::new();
    INSTALLING.get_or_init(|| parking_lot::Mutex::new(HashSet::new()))
}

/// Kick off install + connect on a background thread (so a slow uvx fetch + handshake
/// can't block the agent or be cancelled by the user's next words). `false` = an install
/// for this id is already in flight.
pub(super) fn spawn(id: &str) -> bool {
    if !installing().lock().insert(id.to_string()) {
        return false;
    }
    let id = id.to_string();
    std::thread::spawn(move || {
        match runtime::connect(&id) {
            Ok(count) => {
                registry::mark_installed(&id);
                eprintln!("[mcp] installed + connected '{id}' ({count} tools)");
            }
            Err(error) => eprintln!("[mcp] install '{id}' failed: {error}"),
        }
        installing().lock().remove(&id);
    });
    true
}
