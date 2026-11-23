use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::types::{MAX_HISTORY_LIMIT, MIN_HISTORY_LIMIT};

static RETRY_RUNNING: AtomicBool = AtomicBool::new(false);

fn bounded_limit(config: &crate::config::Config) -> usize {
    config
        .max_history_items
        .clamp(MIN_HISTORY_LIMIT, MAX_HISTORY_LIMIT)
}

pub(super) fn results_per_tool_limit() -> usize {
    crate::APP
        .lock()
        .map(|app| bounded_limit(&app.config))
        .unwrap_or(MAX_HISTORY_LIMIT)
}

/// Delivery already owns its mutex; never wait on the app lock in that order.
pub(super) fn results_per_tool_for_delivery() -> usize {
    if let Ok(app) = crate::APP.try_lock() {
        return bounded_limit(&app.config);
    }
    if RETRY_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        std::thread::spawn(|| {
            if let Ok(app) = crate::APP.lock() {
                let limit = bounded_limit(&app.config);
                drop(app);
                super::request_prune(limit);
            }
            RETRY_RUNNING.store(false, Ordering::Release);
        });
    }
    // The largest allowed value never prunes more than the user's setting.
    MAX_HISTORY_LIMIT
}
