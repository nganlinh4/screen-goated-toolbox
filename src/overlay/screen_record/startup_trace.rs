use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const TRACE_ID_ENV: &str = "SGT_RECORDER_STARTUP_TRACE_ID";
const TRACE_START_ENV: &str = "SGT_RECORDER_STARTUP_STARTED_MS";

#[derive(Clone, Copy)]
struct StartupTrace {
    id: u64,
    started_ms: u64,
}

static TRACE: OnceLock<Option<StartupTrace>> = OnceLock::new();

fn trace() -> Option<StartupTrace> {
    *TRACE.get_or_init(|| {
        Some(StartupTrace {
            id: std::env::var(TRACE_ID_ENV).ok()?.parse().ok()?,
            started_ms: std::env::var(TRACE_START_ENV).ok()?.parse().ok()?,
        })
    })
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

pub(super) fn log(milestone: &str) {
    let Some(trace) = trace() else { return };
    let elapsed_ms = unix_ms().saturating_sub(trace.started_ms);
    crate::log_info!(
        "[RecorderStartup] trace={} milestone={milestone} elapsed_ms={elapsed_ms}",
        trace.id
    );
}
