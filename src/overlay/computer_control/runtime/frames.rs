//! Provenance-aware delivery of model-visible frames.

use super::*;

pub(super) fn capture_cache_needed(
    request_active: bool,
    terminal_drain: bool,
    recent_user_voice: bool,
    action_pending: bool,
    user_voice_now: bool,
) -> bool {
    ((request_active && !terminal_drain) || recent_user_voice)
        && (!action_pending || user_voice_now)
}

pub(super) fn send_snapshot(
    socket: &mut Sock,
    frame: &uia_task::SnapshotFrame,
    trigger: &str,
) -> anyhow::Result<()> {
    send_frame(
        socket,
        &frame.b64,
        &frame.source,
        trigger,
        Some(frame.captured_at.elapsed().as_millis()),
        frame.byte_count,
        None,
    )
}

pub(super) fn send_action_frame(
    socket: &mut Sock,
    b64: &str,
    source: &uia_task::FrameSource,
    action: telemetry::ActionTrace,
) -> anyhow::Result<()> {
    send_frame(
        socket,
        b64,
        source,
        "post_action",
        None,
        b64.len().saturating_mul(3) / 4,
        Some(action),
    )
}

fn send_frame(
    socket: &mut Sock,
    b64: &str,
    source: &uia_task::FrameSource,
    trigger: &str,
    capture_age_ms: Option<u128>,
    byte_count: usize,
    action: Option<telemetry::ActionTrace>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let result = send(socket, realtime_video_jpeg_b64(b64));
    let fields = serde_json::json!({
        "frame_id": source.frame_id,
        "surface": &source.surface,
        "trigger": trigger,
        "capture_age_ms": capture_age_ms,
        "byte_count": byte_count,
        "send_ms": started.elapsed().as_millis(),
        "ok": result.is_ok(),
        "error": result.as_ref().err().map(ToString::to_string),
    });
    match action {
        Some(trace) => {
            telemetry::event_for_action("frame_sent", "runtime", Privacy::Safe, trace, fields)
        }
        None => telemetry::event("frame_sent", "runtime", Privacy::Safe, fields),
    }
    result
}

pub(super) fn capture_failed(trigger: &str, target: Option<&str>, error: &dyn std::fmt::Display) {
    telemetry::typed_error(
        "ERR_FRAME_CAPTURE_FAILED",
        "capture",
        "failed to capture a model-visible frame",
        serde_json::json!({
            "trigger": trigger,
            "target": target,
            "error": error.to_string(),
        }),
    );
}

/// Escalation gate for the best-effort background capture cache. A transient
/// miss self-heals on the next tick, so only a persistent streak — the model is
/// actually blind — becomes typed evidence, re-reported at most every 10s.
pub(super) struct CaptureFailureGate {
    failing_since: Option<std::time::Instant>,
    last_reported: Option<std::time::Instant>,
}

const CAPTURE_PERSISTENT_AFTER: std::time::Duration = std::time::Duration::from_secs(2);
const CAPTURE_REPORT_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

impl CaptureFailureGate {
    pub(super) fn new() -> Self {
        Self {
            failing_since: None,
            last_reported: None,
        }
    }

    pub(super) fn on_success(&mut self) {
        self.failing_since = None;
    }

    /// Returns true when this failure should be reported as a typed error.
    pub(super) fn on_failure(&mut self, now: std::time::Instant) -> bool {
        let since = *self.failing_since.get_or_insert(now);
        if now.duration_since(since) >= CAPTURE_PERSISTENT_AFTER
            && self
                .last_reported
                .is_none_or(|last| now.duration_since(last) >= CAPTURE_REPORT_EVERY)
        {
            self.last_reported = Some(now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CaptureFailureGate, capture_cache_needed};
    use std::time::{Duration, Instant};

    #[test]
    fn terminal_final_stream_does_not_keep_capture_warm_without_recent_voice() {
        assert!(capture_cache_needed(true, false, false, false, false));
        assert!(!capture_cache_needed(true, true, false, false, false));
        assert!(capture_cache_needed(true, true, true, false, false));
        assert!(capture_cache_needed(false, false, true, false, false));
        assert!(!capture_cache_needed(false, false, false, false, false));
    }

    #[test]
    fn in_flight_action_suspends_competing_capture_except_during_live_barge_in() {
        assert!(!capture_cache_needed(true, false, true, true, false));
        assert!(capture_cache_needed(true, false, true, true, true));
    }

    #[test]
    fn only_a_persistent_capture_failure_streak_is_reported() {
        let mut gate = CaptureFailureGate::new();
        let t0 = Instant::now();

        assert!(!gate.on_failure(t0), "a one-off miss is not evidence");
        gate.on_success();
        assert!(
            !gate.on_failure(t0 + Duration::from_secs(3)),
            "recovery resets the streak"
        );

        let mut gate = CaptureFailureGate::new();
        assert!(!gate.on_failure(t0));
        assert!(gate.on_failure(t0 + Duration::from_secs(2)), "persistent");
        assert!(
            !gate.on_failure(t0 + Duration::from_secs(5)),
            "re-reports are rate-limited"
        );
        assert!(gate.on_failure(t0 + Duration::from_secs(13)));
    }
}
