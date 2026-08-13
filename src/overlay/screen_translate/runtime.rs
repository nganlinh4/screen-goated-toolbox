use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use std::sync::atomic::AtomicBool;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_CANCEL: LazyLock<Mutex<Option<Arc<AtomicBool>>>> = LazyLock::new(|| Mutex::new(None));
static ACTIVE_OVERLAY: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(super) fn begin_job() -> (u64, Arc<AtomicBool>) {
    cancel_active();
    let job_id = NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut active) = ACTIVE_CANCEL.lock() {
        *active = Some(Arc::clone(&cancel));
    }
    (job_id, cancel)
}

pub(super) fn cancel_active() {
    NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut active) = ACTIVE_CANCEL.lock()
        && let Some(cancel) = active.take()
    {
        cancel.store(true, Ordering::SeqCst);
    }
    if let Ok(mut active) = ACTIVE_OVERLAY.lock()
        && let Some(chain_id) = active.take()
    {
        crate::overlay::result::close_chain_windows(&chain_id);
    }
}

pub(super) fn is_current(job_id: u64) -> bool {
    NEXT_JOB_ID.load(Ordering::SeqCst) == job_id
}

pub(super) fn register_overlay(job_id: u64, chain_id: String) {
    if is_current(job_id) {
        if let Ok(mut active) = ACTIVE_OVERLAY.lock() {
            *active = Some(chain_id);
        }
    } else {
        crate::overlay::result::close_chain_windows(&chain_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_jobs_cancel_and_stale_older_jobs() {
        let (first_id, first_cancel) = begin_job();
        let (second_id, _) = begin_job();
        assert!(first_cancel.load(Ordering::SeqCst));
        assert!(!is_current(first_id));
        assert!(is_current(second_id));
    }
}
