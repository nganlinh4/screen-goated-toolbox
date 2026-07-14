//! Provenance-aware delivery of model-visible frames.

use super::*;

pub(super) fn capture_cache_needed(
    request_active: bool,
    terminal_drain: bool,
    recent_user_voice: bool,
) -> bool {
    (request_active && !terminal_drain) || recent_user_voice
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

#[cfg(test)]
mod tests {
    use super::capture_cache_needed;

    #[test]
    fn terminal_final_stream_does_not_keep_capture_warm_without_recent_voice() {
        assert!(capture_cache_needed(true, false, false));
        assert!(!capture_cache_needed(true, true, false));
        assert!(capture_cache_needed(true, true, true));
        assert!(capture_cache_needed(false, false, true));
        assert!(!capture_cache_needed(false, false, false));
    }
}
