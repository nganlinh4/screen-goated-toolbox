use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use std::sync::atomic::AtomicBool;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_JOB_ID: AtomicU64 = AtomicU64::new(0);
type OwnedJob = (Arc<AtomicBool>, Option<String>);
static SUBTITLE_JOBS: LazyLock<Mutex<std::collections::HashMap<u64, OwnedJob>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
static JOB_LOCK: Mutex<()> = Mutex::new(());
static ACTIVE_CANCEL: LazyLock<Mutex<Option<Arc<AtomicBool>>>> = LazyLock::new(|| Mutex::new(None));
static ACTIVE_OVERLAY: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub(super) fn begin_job() -> (u64, Arc<AtomicBool>) {
    let _guard = JOB_LOCK.lock().unwrap();
    cancel_locked();
    let job_id = NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst) + 1;
    ACTIVE_JOB_ID.store(job_id, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut active) = ACTIVE_CANCEL.lock() {
        *active = Some(Arc::clone(&cancel));
    }
    (job_id, cancel)
}

pub(super) fn cancel_active() {
    let _guard = JOB_LOCK.lock().unwrap();
    cancel_locked();
}

fn cancel_locked() {
    for (_, (cancel, chain)) in SUBTITLE_JOBS.lock().unwrap().drain() {
        cancel.store(true, Ordering::SeqCst);
        if let Some(chain) = chain {
            crate::overlay::result::close_chain_windows(&chain);
        }
    }
    cancel_primary();
}

fn cancel_primary() {
    ACTIVE_JOB_ID.store(0, Ordering::SeqCst);
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
    ACTIVE_JOB_ID.load(Ordering::SeqCst) == job_id
        || SUBTITLE_JOBS.lock().unwrap().contains_key(&job_id)
}

pub(super) fn is_subtitle(job_id: u64) -> bool {
    SUBTITLE_JOBS.lock().unwrap().contains_key(&job_id)
}

/// Independent text groups share inference but own separate result lifetimes.
pub(super) fn begin_subtitle_job() -> (u64, Arc<AtomicBool>) {
    let _guard = JOB_LOCK.lock().unwrap();
    let id = NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    SUBTITLE_JOBS
        .lock()
        .unwrap()
        .insert(id, (Arc::clone(&cancel), None));
    (id, cancel)
}

/// A session can cancel its own result without closing a newer manual capture.
pub(super) fn cancel_job(job_id: u64) {
    let _guard = JOB_LOCK.lock().unwrap();
    let owned = SUBTITLE_JOBS.lock().unwrap().remove(&job_id);
    if let Some((cancel, chain)) = owned {
        cancel.store(true, Ordering::SeqCst);
        if let Some(chain) = chain {
            crate::overlay::result::close_chain_windows(&chain);
        }
    } else if ACTIVE_JOB_ID.load(Ordering::SeqCst) == job_id {
        cancel_primary();
    }
}

pub(super) fn register_overlay(job_id: u64, chain_id: String) {
    let _guard = JOB_LOCK.lock().unwrap();
    if let Some((_, chain)) = SUBTITLE_JOBS.lock().unwrap().get_mut(&job_id) {
        *chain = Some(chain_id);
        return;
    }
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
        cancel_job(first_id);
        assert!(is_current(second_id));
        cancel_job(second_id);
        assert!(!is_current(second_id));
        let (left, left_cancel) = begin_subtitle_job();
        let (right, right_cancel) = begin_subtitle_job();
        assert!(is_current(left) && is_current(right));
        cancel_job(left);
        assert!(left_cancel.load(Ordering::SeqCst));
        assert!(!right_cancel.load(Ordering::SeqCst));
        assert!(is_current(right));
        let (manual, _) = begin_job();
        assert!(right_cancel.load(Ordering::SeqCst));
        cancel_job(right);
        assert!(is_current(manual));
        cancel_job(manual);
    }
}
