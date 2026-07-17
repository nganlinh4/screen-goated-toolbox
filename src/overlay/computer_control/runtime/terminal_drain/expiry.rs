//! Idle fallback for terminal generations whose protocol boundary is missing.

use super::*;

pub(super) const FINAL_RESPONSE_IDLE_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(20);

/// Call only after a nonblocking socket read reports that its inbound queue is
/// empty. Already-queued model output and advancing playback refresh activity
/// before a missing-boundary fallback can close the response.
pub(crate) fn expire_after_socket_drained(state: &mut Reader, sink: Option<&AudioSink>) {
    refresh_playback_activity(state, sink);
    if !state.terminal_response.is_open()
        || state
            .terminal_activity_at
            .is_none_or(|activity| activity.elapsed() < FINAL_RESPONSE_IDLE_TIMEOUT)
    {
        return;
    }
    let (code, message, reason) = match state.terminal_response {
        FinalResponseState::Streaming if state.terminal_generation_complete => (
            "ERR_TERMINAL_TURN_BOUNDARY_MISSING",
            "terminal generation completed and playback stopped advancing without a turn boundary",
            "turn_boundary_timeout",
        ),
        FinalResponseState::Streaming => (
            "ERR_TERMINAL_FINAL_BOUNDARY_MISSING",
            "terminal final response and playback stopped advancing without a closing boundary",
            "stream_boundary_timeout",
        ),
        FinalResponseState::AwaitingPriorBoundary | FinalResponseState::AwaitingOutput => (
            "ERR_TERMINAL_FINAL_OUTPUT_MISSING",
            "accepted terminal generation produced no final output before timeout",
            "empty_response_timeout",
        ),
        FinalResponseState::Closed => return,
    };
    telemetry::typed_error(
        code,
        "runtime",
        message,
        serde_json::json!({
            "accepted": state.terminal_accepted,
            "state": format!("{:?}", state.terminal_response),
            "timeout_ms": FINAL_RESPONSE_IDLE_TIMEOUT.as_millis(),
        }),
    );
    let response_completed = state.terminal_generation_complete
        && state.terminal_response == FinalResponseState::Streaming;
    close_final_response(state, reason, false, response_completed);
}

fn refresh_playback_activity(state: &mut Reader, sink: Option<&AudioSink>) {
    // Playback may belong to the function-call generation while its terminal
    // tool response is already waiting for the final generation. A long spoken
    // progress update must not make that final-response wait look abandoned.
    if !state.terminal_response.is_open() {
        return;
    }
    let Some(sink) = sink else {
        return;
    };
    record_playback_observation(state, sink.played_samples(), sink.queued_samples());
}

fn record_playback_observation(state: &mut Reader, played: u64, queued: usize) {
    let advanced = match state.terminal_playback_cursor {
        // A cumulative sink counter can include old utterances. On the first
        // observation only a live queue proves current playback activity.
        None => queued > 0,
        Some(previous) if played > previous => true,
        // Rebuilding the output device replaces the sink and resets its
        // cumulative counter. A lower cursor plus queued audio is progress on
        // the replacement sink, not a permanent stall.
        Some(previous) if played < previous => queued > 0,
        Some(_) => false,
    };
    state.terminal_playback_cursor = Some(played);
    if advanced {
        state.terminal_activity_at = Some(std::time::Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advancing_playback_refreshes_terminal_activity_but_a_stall_does_not() {
        let stale = std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT;
        let mut state = Reader {
            terminal_response: FinalResponseState::Streaming,
            terminal_activity_at: Some(stale),
            ..Reader::default()
        };

        record_playback_observation(&mut state, 10, 400);
        let first_refresh = state.terminal_activity_at.unwrap();
        assert!(first_refresh > stale);

        state.terminal_activity_at = Some(stale);
        record_playback_observation(&mut state, 10, 400);
        assert_eq!(state.terminal_activity_at, Some(stale));

        record_playback_observation(&mut state, 20, 200);
        assert!(state.terminal_activity_at.unwrap() > stale);
    }

    #[test]
    fn replacement_sink_counter_reset_with_queued_audio_is_progress() {
        let stale = std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT;
        let mut state = Reader {
            terminal_response: FinalResponseState::Streaming,
            terminal_activity_at: Some(stale),
            terminal_playback_cursor: Some(10_000),
            ..Reader::default()
        };

        record_playback_observation(&mut state, 0, 400);

        assert_eq!(state.terminal_playback_cursor, Some(0));
        assert!(state.terminal_activity_at.unwrap() > stale);
    }

    #[test]
    fn first_observation_requires_a_live_queue_not_an_old_global_counter() {
        let stale = std::time::Instant::now() - FINAL_RESPONSE_IDLE_TIMEOUT;
        let mut state = Reader {
            terminal_response: FinalResponseState::AwaitingOutput,
            terminal_activity_at: Some(stale),
            ..Reader::default()
        };

        record_playback_observation(&mut state, 50_000, 0);
        assert_eq!(state.terminal_activity_at, Some(stale));

        record_playback_observation(&mut state, 50_000, 400);
        assert_eq!(state.terminal_activity_at, Some(stale));

        record_playback_observation(&mut state, 50_100, 300);
        assert!(state.terminal_activity_at.unwrap() > stale);
    }
}
