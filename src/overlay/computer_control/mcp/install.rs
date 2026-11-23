use super::{registry, runtime};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Install threads in flight, keyed by integration id, with each thread's own
/// stop flag (so the UI shows "installing…" and won't double-spawn).
pub(super) fn installing() -> &'static parking_lot::Mutex<HashMap<String, Arc<AtomicBool>>> {
    static INSTALLING: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, Arc<AtomicBool>>>> =
        std::sync::OnceLock::new();
    INSTALLING.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Kick off install + connect on a background thread. Ordinary agent turns do
/// not own this operation; Downloaded Tools cleanup does.
pub(super) fn spawn(id: &str) -> bool {
    let Some(stop) = begin(id) else {
        return false;
    };
    let id = id.to_string();
    let Ok(activity) = crate::install_activity::register(stop.clone()) else {
        installing().lock().remove(&id);
        return false;
    };
    std::thread::spawn(super::super::session_retention::inherit(move || {
        let _activity = activity;
        let outcome = runtime::connect_owned(&id, &stop);
        finish(&id, &stop, outcome);
    }));
    true
}

fn begin(id: &str) -> Option<Arc<AtomicBool>> {
    let mut installing = installing().lock();
    if installing.contains_key(id) {
        return None;
    }
    let stop = Arc::new(AtomicBool::new(false));
    installing.insert(id.to_string(), stop.clone());
    Some(stop)
}

/// Stop in-flight installs before a removal, so a late result can neither
/// register a connection nor re-record an integration the user removed.
pub(super) fn cancel(id: &str) {
    if let Some(stop) = installing().lock().remove(id) {
        stop.store(true, Ordering::SeqCst);
    }
}

pub(super) fn cancel_all() {
    for (_, stop) in installing().lock().drain() {
        stop.store(true, Ordering::SeqCst);
    }
}

/// Record the outcome under the same lock `cancel` takes, so a removal either
/// precedes this check or follows the registry write and undoes it.
fn finish(id: &str, stop: &Arc<AtomicBool>, outcome: Result<usize, String>) {
    let mut installing = installing().lock();
    match outcome {
        Ok(count) if !stop.load(Ordering::SeqCst) => {
            registry::mark_installed(id);
            cc_diagnostic!("[mcp] installed + connected '{id}' ({count} tools)");
        }
        Ok(_) => cc_diagnostic!("[mcp] install '{id}' was cancelled"),
        Err(error) => cc_diagnostic!("[mcp] install '{id}' failed: {error}"),
    }
    if installing
        .get(id)
        .is_some_and(|owner| Arc::ptr_eq(owner, stop))
    {
        installing.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_integration_removed_while_installing_stays_removed() {
        let id = "ccfree-removed-during-install";
        let stop = begin(id).expect("install admitted");

        super::super::remove_tool(id);
        finish(id, &stop, Ok(1));

        let reinstalled = registry::is_installed(id);
        registry::remove(id);
        assert!(
            !reinstalled,
            "a late install result re-registered a removed integration"
        );
        assert!(!installing().lock().contains_key(id));
    }

    #[test]
    fn a_cancelled_install_cannot_retire_its_replacement() {
        let id = "ccfree-reinstalled-after-removal";
        let stale = begin(id).expect("first install admitted");
        cancel(id);
        let current = begin(id).expect("a new install is admitted after removal");

        finish(id, &stale, Ok(1));

        assert!(!registry::is_installed(id));
        assert!(
            installing()
                .lock()
                .get(id)
                .is_some_and(|owner| Arc::ptr_eq(owner, &current))
        );
        cancel(id);
    }
}
