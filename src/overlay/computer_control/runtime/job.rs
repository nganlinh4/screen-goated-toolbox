use super::*;

pub(super) const FRAME_INTERVAL: Duration = Duration::from_millis(1800);
pub(super) const CAPTURE_CACHE_INTERVAL: Duration = Duration::from_millis(500);
pub(super) const MAX_RECONNECTS: u32 = 6;
pub(super) const NUDGE_SILENCE: Duration = Duration::from_secs(8);
pub(super) const RECONNECT_SILENCE: Duration = Duration::from_secs(40);
/// Internal cleanup does not produce a model response.
pub(super) const RETIRE_TURN: &str = "__retire_turn__";

/// Executor work owns cancellation so late results cannot resurrect cancelled jobs.
pub(super) struct Job {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) args: Value,
    pub(super) task: String,
    pub(super) user_text: String,
    pub(super) inherit_evidence: bool,
    pub(super) action: telemetry::ActionTrace,
    pub(super) source_frame: Option<uia_task::FrameSource>,
    pub(super) queued_at: Instant,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) type Done = (
    String,
    String,
    Value,
    Option<(String, uia_task::FrameSource)>,
    Arc<AtomicBool>,
    telemetry::ActionTrace,
    bool, // The requested dispatch did not start; turn cleanup may still have run.
);
