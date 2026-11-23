use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use std::sync::atomic::AtomicBool;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_JOB_ID: AtomicU64 = AtomicU64::new(0);
type OwnedJob = (Arc<AtomicBool>, Option<String>);
static SUBTITLE_JOBS: LazyLock<Mutex<std::collections::HashMap<u64, OwnedJob>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
static JOB_LOCK: Mutex<()> = Mutex::new(());
#[cfg(debug_assertions)]
static JOB_ACTIVITY: AtomicU64 = AtomicU64::new(0);
#[cfg(debug_assertions)]
static JOB_START_LOCK: Mutex<()> = Mutex::new(());
static ACTIVE_CANCEL: LazyLock<Mutex<Option<Arc<AtomicBool>>>> = LazyLock::new(|| Mutex::new(None));
static ACTIVE_OVERLAY: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static FAILED_JOBS: LazyLock<Mutex<std::collections::HashMap<u64, String>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
const RETAINED_FAILURES: u64 = 32;
type RendererWakes =
    std::collections::HashMap<u64, std::sync::Weak<crate::overlay::wake_event::WakeEvent>>;
static RENDERER_WAKES: LazyLock<Mutex<RendererWakes>> =
    LazyLock::new(|| Mutex::new(RendererWakes::new()));

pub(super) struct RendererWake {
    job_id: u64,
    wake: std::sync::Weak<crate::overlay::wake_event::WakeEvent>,
}

pub(super) fn register_renderer_wake(
    job_id: u64,
    wake: &Arc<crate::overlay::wake_event::WakeEvent>,
) -> Option<RendererWake> {
    let _job = JOB_LOCK.lock().unwrap();
    if !is_current(job_id) {
        return None;
    }
    let mut wakes = RENDERER_WAKES.lock().unwrap();
    if wakes
        .get(&job_id)
        .and_then(std::sync::Weak::upgrade)
        .is_some()
    {
        return None;
    }
    let wake = Arc::downgrade(wake);
    wakes.insert(job_id, wake.clone());
    Some(RendererWake { job_id, wake })
}

impl Drop for RendererWake {
    fn drop(&mut self) {
        let mut wakes = RENDERER_WAKES.lock().unwrap();
        if wakes
            .get(&self.job_id)
            .is_some_and(|wake| wake.ptr_eq(&self.wake))
        {
            wakes.remove(&self.job_id);
        }
    }
}

fn wake_renderer(job_id: u64) {
    if let Some(wake) = RENDERER_WAKES
        .lock()
        .unwrap()
        .get(&job_id)
        .and_then(std::sync::Weak::upgrade)
    {
        wake.signal();
    }
}

pub(super) fn begin_job() -> (u64, Arc<AtomicBool>) {
    #[cfg(debug_assertions)]
    let _start = JOB_START_LOCK.lock().unwrap();
    let _guard = JOB_LOCK.lock().unwrap();
    begin_job_locked()
}

fn begin_job_locked() -> (u64, Arc<AtomicBool>) {
    #[cfg(debug_assertions)]
    mark_job_activity();
    cancel_primary();
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
    #[cfg(debug_assertions)]
    mark_job_activity();
    cancel_locked();
}

fn cancel_locked() {
    for (id, (cancel, chain)) in SUBTITLE_JOBS.lock().unwrap().drain() {
        cancel.store(true, Ordering::SeqCst);
        wake_renderer(id);
        if let Some(chain) = chain {
            crate::overlay::result::close_chain_windows(&chain);
        }
    }
    cancel_primary();
}

fn cancel_primary() {
    let retired = ACTIVE_JOB_ID.swap(0, Ordering::SeqCst);
    wake_renderer(retired);
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
    #[cfg(debug_assertions)]
    mark_job_activity();
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
    cancel_job_locked(job_id);
}

/// Ends a job because one of its components failed, not because the user
/// stopped it. A live job keeps the cause for its owner to report.
pub(super) fn fail_job(job_id: u64, cause: &str) {
    let _guard = JOB_LOCK.lock().unwrap();
    if is_current(job_id) {
        record_failure(&mut FAILED_JOBS.lock().unwrap(), job_id, cause);
    }
    cancel_job_locked(job_id);
}

fn record_failure(failures: &mut std::collections::HashMap<u64, String>, job_id: u64, cause: &str) {
    // Ids are monotonic; an owner that never collects its cause cannot grow this.
    failures.retain(|id, _| id.saturating_add(RETAINED_FAILURES) > job_id);
    failures.insert(job_id, cause.to_string());
}

/// How a primary job ended, as its owner must report it.
#[derive(Debug)]
pub(super) enum JobEnd {
    Completed,
    /// Stopped by the user or superseded by a newer job: silent.
    Cancelled,
    /// The job is still current and failed: reported unless superseded.
    Failed(anyhow::Error),
    /// A component ended the job with this cause: always reported.
    ComponentFailed(String),
}

pub(super) fn end_job(job_id: u64, cancel: &AtomicBool, result: anyhow::Result<()>) -> JobEnd {
    let recorded = FAILED_JOBS.lock().unwrap().remove(&job_id);
    let live = is_current(job_id) && !cancel.load(Ordering::SeqCst);
    classify_end(result, recorded, live)
}

fn classify_end(result: anyhow::Result<()>, recorded: Option<String>, live: bool) -> JobEnd {
    match (recorded, result) {
        (Some(cause), _) => JobEnd::ComponentFailed(cause),
        (None, Ok(())) => JobEnd::Completed,
        (None, Err(error)) if live => JobEnd::Failed(error),
        (None, Err(_)) => JobEnd::Cancelled,
    }
}

fn cancel_job_locked(job_id: u64) {
    #[cfg(debug_assertions)]
    mark_job_activity();
    let owned = SUBTITLE_JOBS.lock().unwrap().remove(&job_id);
    if let Some((cancel, chain)) = owned {
        cancel.store(true, Ordering::SeqCst);
        wake_renderer(job_id);
        if let Some(chain) = chain {
            crate::overlay::result::close_chain_windows(&chain);
        }
    } else if ACTIVE_JOB_ID.load(Ordering::SeqCst) == job_id {
        cancel_primary();
    }
}

#[cfg(debug_assertions)]
fn mark_job_activity() {
    let _ = JOB_ACTIVITY.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
        Some(value.saturating_add(1))
    });
}

#[cfg(debug_assertions)]
pub(super) fn lab_activity_snapshot() -> Option<u64> {
    let _guard = JOB_LOCK.lock().ok()?;
    let activity = JOB_ACTIVITY.load(Ordering::SeqCst);
    (activity != u64::MAX).then_some(activity)
}

#[cfg(debug_assertions)]
pub(super) fn begin_lab_job(expected: u64) -> Option<(u64, Arc<AtomicBool>)> {
    let _start = JOB_START_LOCK.lock().ok()?;
    let _guard = JOB_LOCK.lock().ok()?;
    (expected != u64::MAX && JOB_ACTIVITY.load(Ordering::SeqCst) == expected).then(begin_job_locked)
}

#[cfg(debug_assertions)]
pub(super) fn with_lab_start_guard<T>(expected: u64, effect: impl FnOnce() -> T) -> Option<T> {
    let _start = JOB_START_LOCK.lock().ok()?;
    {
        let _guard = JOB_LOCK.lock().ok()?;
        if expected == u64::MAX || JOB_ACTIVITY.load(Ordering::SeqCst) != expected {
            return None;
        }
    }
    Some(effect())
}

pub(super) fn with_current_primary<T>(job_id: u64, effect: impl FnOnce() -> T) -> Option<T> {
    let _guard = JOB_LOCK.lock().ok()?;
    (ACTIVE_JOB_ID.load(Ordering::SeqCst) == job_id).then(effect)
}

#[cfg(debug_assertions)]
pub(super) fn with_lab_activity<T>(expected: u64, effect: impl FnOnce() -> T) -> Option<T> {
    let _guard = JOB_LOCK.lock().ok()?;
    (expected != u64::MAX && JOB_ACTIVITY.load(Ordering::SeqCst) == expected).then(effect)
}

/// Publishes visible output only while the job is current, and registers its
/// overlay in the same critical section: a concurrent cancel either prevents
/// the publication or finds the overlay registered and closes it.
pub(super) fn publish_overlay(job_id: u64, chain_id: &str, publish: impl FnOnce() -> bool) -> bool {
    let _guard = JOB_LOCK.lock().unwrap();
    if !is_current(job_id) || !publish() {
        return false;
    }
    if let Some((_, chain)) = SUBTITLE_JOBS.lock().unwrap().get_mut(&job_id) {
        *chain = Some(chain_id.to_string());
    } else if let Ok(mut active) = ACTIVE_OVERLAY.lock() {
        *active = Some(chain_id.to_string());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_failure_is_a_reported_end_and_user_cancel_stays_silent() {
        let failed = classify_end(
            Err(anyhow::anyhow!("screen translation was cancelled")),
            Some("renderer mailbox failed".into()),
            false,
        );
        assert!(
            matches!(failed, JobEnd::ComponentFailed(cause) if cause == "renderer mailbox failed")
        );
        assert!(matches!(
            classify_end(Ok(()), Some("late".into()), false),
            JobEnd::ComponentFailed(_)
        ));
        assert!(matches!(
            classify_end(Err(anyhow::anyhow!("cancelled")), None, false),
            JobEnd::Cancelled
        ));
        assert!(matches!(
            classify_end(Err(anyhow::anyhow!("provider failed")), None, true),
            JobEnd::Failed(error) if error.to_string() == "provider failed"
        ));
        assert!(matches!(
            classify_end(Ok(()), None, true),
            JobEnd::Completed
        ));
    }

    #[test]
    fn uncollected_failures_are_bounded() {
        let mut failures = std::collections::HashMap::new();
        for job_id in 1..=RETAINED_FAILURES * 3 {
            record_failure(&mut failures, job_id, "uncollected");
        }
        assert_eq!(failures.len() as u64, RETAINED_FAILURES);
        assert!(failures.contains_key(&(RETAINED_FAILURES * 3)));
    }

    #[test]
    fn newer_jobs_cancel_and_stale_older_jobs() {
        let (first_id, first_cancel) = begin_job();
        assert!(is_current(first_id));
        let (second_id, _) = begin_job();
        assert!(
            with_current_primary(first_id, || panic!("stale notification was published")).is_none()
        );
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
        assert!(!right_cancel.load(Ordering::SeqCst));
        assert!(is_current(right));
        cancel_job(right);
        assert!(right_cancel.load(Ordering::SeqCst));
        assert!(is_current(manual));
        #[cfg(debug_assertions)]
        {
            let deferred = lab_activity_snapshot().unwrap();
            let (newer, newer_cancel) = begin_job();
            assert!(begin_lab_job(deferred).is_none());
            assert!(is_current(newer));
            assert!(!newer_cancel.load(Ordering::SeqCst));
            cancel_job(newer);
            let expected = lab_activity_snapshot().unwrap();
            let (entered_tx, entered_rx) = std::sync::mpsc::channel();
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let worker = with_lab_start_guard(expected, || {
                let worker = std::thread::spawn(move || {
                    entered_tx.send(()).unwrap();
                    let (job, _) = begin_job();
                    started_tx.send(job).unwrap();
                });
                entered_rx.recv().unwrap();
                assert!(
                    started_rx
                        .recv_timeout(std::time::Duration::from_millis(30))
                        .is_err()
                );
                worker
            })
            .unwrap();
            let job = started_rx.recv().unwrap();
            worker.join().unwrap();
            cancel_job(job);
            let (old, _) = begin_job();
            let old_activity = lab_activity_snapshot().unwrap();
            let (resume, wait) = std::sync::mpsc::channel();
            let delayed = std::thread::spawn(move || {
                wait.recv().unwrap();
                with_current_primary(old, || panic!("stale capture was published"))
            });
            let (newer, _) = begin_job();
            resume.send(()).unwrap();
            assert!(delayed.join().unwrap().is_none());
            assert!(
                with_lab_activity(old_activity, || panic!(
                    "stale pre-admission error was published"
                ))
                .is_none()
            );
            assert!(with_current_primary(newer, || ()).is_some());
            cancel_job(newer);
        }
        cancel_job(manual);
        let (subtitle, subtitle_cancel) = begin_subtitle_job();
        let (primary, primary_cancel) = begin_job();
        cancel_active();
        assert!(subtitle_cancel.load(Ordering::SeqCst));
        assert!(primary_cancel.load(Ordering::SeqCst));
        assert!(!is_current(subtitle));
        assert!(!is_current(primary));
        let (left, _) = begin_subtitle_job();
        let (right, _) = begin_subtitle_job();
        let (primary, _) = begin_job();
        let left_wake = Arc::new(crate::overlay::wake_event::WakeEvent::new().unwrap());
        let right_wake = Arc::new(crate::overlay::wake_event::WakeEvent::new().unwrap());
        let primary_wake = Arc::new(crate::overlay::wake_event::WakeEvent::new().unwrap());
        let left_registration = register_renderer_wake(left, &left_wake).unwrap();
        let right_registration = register_renderer_wake(right, &right_wake).unwrap();
        let primary_registration = register_renderer_wake(primary, &primary_wake).unwrap();
        assert!(register_renderer_wake(primary, &left_wake).is_none());
        cancel_job(left);
        assert!(left_wake.signalled_within(100));
        assert!(!right_wake.signalled_within(0));
        assert!(!primary_wake.signalled_within(0));
        assert!(register_renderer_wake(left, &left_wake).is_none());
        let (newer, _) = begin_job();
        assert!(primary_wake.signalled_within(100));
        assert!(!right_wake.signalled_within(0));
        cancel_active();
        assert!(right_wake.signalled_within(100));
        drop((left_registration, right_registration, primary_registration));
        assert!(RENDERER_WAKES.lock().unwrap().is_empty());
        assert!(!is_current(newer));
        // Global job state is shared, so failure ownership is checked in this test too.
        let (failed, failed_cancel) = begin_job();
        fail_job(failed, "renderer mailbox failed");
        assert!(failed_cancel.load(Ordering::SeqCst));
        assert!(!is_current(failed));
        let end = end_job(failed, &failed_cancel, Err(anyhow::anyhow!("cancelled")));
        assert!(
            matches!(end, JobEnd::ComponentFailed(cause) if cause == "renderer mailbox failed")
        );
        // The cause is consumed by its one owner.
        let end = end_job(failed, &failed_cancel, Err(anyhow::anyhow!("cancelled")));
        assert!(matches!(end, JobEnd::Cancelled));
        let (cancelled, cancelled_flag) = begin_job();
        cancel_job(cancelled);
        let end = end_job(
            cancelled,
            &cancelled_flag,
            Err(anyhow::anyhow!("cancelled")),
        );
        assert!(matches!(end, JobEnd::Cancelled));
        // A subtitle job's renderer thread owns and consumes its cause.
        let (subtitle, subtitle_cancel) = begin_subtitle_job();
        fail_job(subtitle, "subtitle renderer failed");
        assert!(subtitle_cancel.load(Ordering::SeqCst));
        let end = end_job(subtitle, &subtitle_cancel, Err(anyhow::anyhow!("stopped")));
        assert!(
            matches!(end, JobEnd::ComponentFailed(cause) if cause == "subtitle renderer failed")
        );
        assert!(!FAILED_JOBS.lock().unwrap().contains_key(&subtitle));
        // Publication and overlay registration are one step with cancellation.
        let (published, _) = begin_job();
        assert!(publish_overlay(published, "chain-published", || true));
        assert_eq!(
            ACTIVE_OVERLAY.lock().unwrap().as_deref(),
            Some("chain-published")
        );
        assert!(!publish_overlay(published, "chain-unchanged", || false));
        assert_eq!(
            ACTIVE_OVERLAY.lock().unwrap().as_deref(),
            Some("chain-published")
        );
        let (subtitle, _) = begin_subtitle_job();
        assert!(publish_overlay(subtitle, "chain-subtitle", || true));
        assert_eq!(
            SUBTITLE_JOBS.lock().unwrap()[&subtitle].1.as_deref(),
            Some("chain-subtitle")
        );
        cancel_job(subtitle);
        assert!(!publish_overlay(subtitle, "chain-stale", || panic!(
            "a cancelled subtitle job published output"
        )));
        let (newer, _) = begin_job();
        assert!(ACTIVE_OVERLAY.lock().unwrap().is_none());
        assert!(!publish_overlay(published, "chain-stale", || panic!(
            "a superseded job published output"
        )));
        // A cancel that arrives while output is being published waits for the
        // registration and then closes that overlay.
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let publisher = std::thread::spawn(move || {
            publish_overlay(newer, "chain-racing", move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                true
            })
        });
        entered_rx.recv().unwrap();
        let canceller = std::thread::spawn(move || cancel_job(newer));
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(is_current(newer), "cancel must wait for the publication");
        release_tx.send(()).unwrap();
        assert!(publisher.join().unwrap());
        canceller.join().unwrap();
        assert!(!is_current(newer));
        assert!(ACTIVE_OVERLAY.lock().unwrap().is_none());
    }
}
