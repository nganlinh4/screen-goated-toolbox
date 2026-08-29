use super::{registry, runtime};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Ids whose install thread is in flight (so the UI shows "installing…" and won't
/// double-spawn).
pub(super) fn installing() -> &'static parking_lot::Mutex<HashSet<String>> {
    static INSTALLING: std::sync::OnceLock<parking_lot::Mutex<HashSet<String>>> =
        std::sync::OnceLock::new();
    INSTALLING.get_or_init(|| parking_lot::Mutex::new(HashSet::new()))
}

/// Kick off install + connect on a background thread. Ordinary agent turns do
/// not own this operation; Downloaded Tools cleanup does.
pub(super) fn spawn(id: &str) -> bool {
    if !installing().lock().insert(id.to_string()) {
        return false;
    }
    let id = id.to_string();
    let stop = Arc::new(AtomicBool::new(false));
    let Ok(activity) = crate::install_activity::register(stop.clone()) else {
        installing().lock().remove(&id);
        return false;
    };
    std::thread::spawn(move || {
        let _activity = activity;
        match runtime::connect_owned(&id, &stop) {
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
