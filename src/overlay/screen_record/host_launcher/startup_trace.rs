use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const TRACE_ID_ENV: &str = "SGT_RECORDER_STARTUP_TRACE_ID";
pub(super) const TRACE_START_ENV: &str = "SGT_RECORDER_STARTUP_STARTED_MS";

static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_TRACE_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_STARTED_MS: AtomicU64 = AtomicU64::new(0);

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

pub(super) fn begin() {
    let started_ms = unix_ms();
    let sequence = NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed);
    let trace_id = started_ms
        .saturating_mul(1_000)
        .saturating_add(sequence % 1_000);
    ACTIVE_TRACE_ID.store(trace_id, Ordering::Release);
    ACTIVE_STARTED_MS.store(started_ms, Ordering::Release);
    log("host-dispatch");
}

pub(super) fn log(milestone: &str) {
    let trace_id = ACTIVE_TRACE_ID.load(Ordering::Acquire);
    let started_ms = ACTIVE_STARTED_MS.load(Ordering::Acquire);
    let elapsed_ms = unix_ms().saturating_sub(started_ms);
    crate::log_info!(
        "[RecorderStartup] trace={trace_id} milestone={milestone} elapsed_ms={elapsed_ms}"
    );
}

pub(super) fn configure(command: &mut Command) {
    command
        .env(
            TRACE_ID_ENV,
            ACTIVE_TRACE_ID.load(Ordering::Acquire).to_string(),
        )
        .env(
            TRACE_START_ENV,
            ACTIVE_STARTED_MS.load(Ordering::Acquire).to_string(),
        );
}
