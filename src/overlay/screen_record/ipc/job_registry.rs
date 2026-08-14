//! Shared scaffolding for the long-running IPC job registries (subtitle
//! generation/translation, the narration jobs, and Gemini Translate narration).
//!
//! Only the genuinely-identical plumbing lives here: the per-job handle, the
//! `OnceLock<Mutex<HashMap>>` accessor, the "find an active job" scan, and the
//! job-id generator. Each handler keeps its own snapshot type, results-revision
//! delta rules, cancel side effects, and `run_job` logic because those diverge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Per-job handle shared across every registry: the live snapshot plus a
/// cancellation flag. Generic over the handler-specific snapshot type.
#[derive(Clone)]
pub(super) struct JobHandle<S> {
    pub(super) snapshot: Arc<Mutex<S>>,
    pub(super) cancelled: Arc<AtomicBool>,
}

/// Snapshot types that expose their lifecycle `state` so the active-job scan can
/// be shared. Every job snapshot already stores `state: String`.
pub(super) trait JobState {
    fn state(&self) -> &str;
}

/// `get_or_init` accessor for a registry's static `OnceLock<Mutex<HashMap>>`.
pub(super) fn registry<S>(
    cell: &'static OnceLock<Mutex<HashMap<String, JobHandle<S>>>>,
) -> &'static Mutex<HashMap<String, JobHandle<S>>> {
    cell.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the id of the first queued/running job, if any. Identical scan used
/// by every registry that enforces a single active job.
pub(super) fn find_active<S: JobState>(jobs: &HashMap<String, JobHandle<S>>) -> Option<String> {
    jobs.iter().find_map(|(job_id, handle)| {
        let snapshot = handle.snapshot.lock().ok()?;
        matches!(snapshot.state(), "queued" | "running").then(|| job_id.clone())
    })
}

const MAX_RETAINED_JOBS: usize = 64;

/// Evicts the oldest terminal snapshots before a new job is inserted. Active
/// jobs are never evicted, and a fully-active registry fails closed.
pub(super) fn prepare_for_insert<S: JobState>(
    jobs: &mut HashMap<String, JobHandle<S>>,
) -> Result<(), String> {
    if jobs.len() < MAX_RETAINED_JOBS {
        return Ok(());
    }
    let mut terminal_ids = jobs
        .iter()
        .filter_map(|(job_id, handle)| {
            let snapshot = handle.snapshot.lock().ok()?;
            (!matches!(snapshot.state(), "queued" | "running")).then(|| job_id.clone())
        })
        .collect::<Vec<_>>();
    terminal_ids.sort_unstable();
    let remove_count = jobs
        .len()
        .saturating_sub(MAX_RETAINED_JOBS.saturating_sub(1));
    for job_id in terminal_ids.into_iter().take(remove_count) {
        jobs.remove(&job_id);
    }
    if jobs.len() >= MAX_RETAINED_JOBS {
        return Err(format!(
            "Job registry is full with {MAX_RETAINED_JOBS} active or locked jobs"
        ));
    }
    Ok(())
}

pub(super) fn cancel_all<S>(jobs: &HashMap<String, JobHandle<S>>) {
    for handle in jobs.values() {
        handle.cancelled.store(true, Ordering::SeqCst);
    }
}

/// Builds a `{prefix}-{millis}-{pid}` job id. `millis` is the current Unix time
/// in milliseconds (same value the previous per-handler generators produced).
pub(super) fn uuid(prefix: &str) -> String {
    static NONCE: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    )
}
