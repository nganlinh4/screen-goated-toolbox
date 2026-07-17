//! Drain model-originated events after the current generation is closed.

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::protocol::ServerEvent;
use super::super::telemetry::{self, Privacy};
use super::reader::{Reader, flush_reply};

mod expiry;
#[cfg(test)]
use expiry::FINAL_RESPONSE_IDLE_TIMEOUT;
pub(super) use expiry::expire_after_socket_drained;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum FinalResponseState {
    #[default]
    Closed,
    /// The tool response was delivered before the boundary for the generation
    /// that produced the tool call. Final output is still accepted immediately:
    /// some Live generations stream it without an intermediate boundary.
    AwaitingPriorBoundary,
    AwaitingOutput,
    Streaming,
}

impl FinalResponseState {
    pub(super) fn is_open(self) -> bool {
        self != Self::Closed
    }

    fn accepts_output(self) -> bool {
        matches!(
            self,
            Self::AwaitingPriorBoundary | Self::AwaitingOutput | Self::Streaming
        )
    }
}

/// A terminal tool response is not itself assistant speech. Keep exactly its
/// owning final generation open, while the drain rejects every further tool.
pub(super) fn begin_final_response(state: &mut Reader, accepted: bool) {
    let prior_boundary_seen = std::mem::take(&mut state.pending_tool_boundary_seen);
    let now = std::time::Instant::now();
    flush_reply(state);
    super::reader_policy::begin_terminal_drain(state, accepted, false);
    state.generation_output_seen = false;
    state.terminal_final_response_delivered = false;
    state.terminal_prior_turn_boundary_pending = false;
    // Sending the terminal tool response owns all following assistant output.
    // The function-call boundary may already have arrived, may still be queued,
    // or may be omitted before final output. Track that transport fact without
    // withholding speech in any of those protocol orderings.
    state.terminal_response = if prior_boundary_seen {
        FinalResponseState::AwaitingOutput
    } else {
        FinalResponseState::AwaitingPriorBoundary
    };
    state.active = true;
    state.awaiting = true;
    // A missing final sentence may be superseded by the user. It must never
    // manufacture a nudge/reconnect that could create a second response.
    state.recovery_owed = false;
    state.think_start = Some(now);
    state.terminal_activity_at = Some(now);
    telemetry::event(
        "terminal_final_response_opened",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "accepted": accepted,
            "ownership": "post_tool_response",
            "prior_boundary_seen": prior_boundary_seen,
        }),
    );
}

/// A state-changing completion may speak its one final response immediately
/// before `done`. Accepting the terminal receipt releases that buffered response
/// and closes the turn without asking the model to answer again.
pub(super) fn finish_pre_tool_response(state: &mut Reader) {
    let prior_boundary_seen = std::mem::take(&mut state.pending_tool_boundary_seen);
    finish_terminal_response(state, prior_boundary_seen, "pre_tool_response");
}

fn finish_terminal_response(state: &mut Reader, boundary_seen: bool, reason: &str) {
    flush_reply(state);
    super::reader_policy::begin_terminal_drain(state, true, boundary_seen);
    state.terminal_final_response_delivered = true;
    telemetry::event(
        "terminal_final_response_closed",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "accepted": true,
            "reason": reason,
            "generation_complete_seen": boundary_seen,
            "turn_boundary_seen": boundary_seen,
            "response_completed": true,
            "dropped_events": 0,
            "effectful_dropped_events": 0,
        }),
    );
}

pub(super) fn handle(event: &ServerEvent, sink: Option<&AudioSink>, state: &mut Reader) -> bool {
    if !state.terminal_drain {
        return false;
    }
    match event {
        // A real user turn owns the session immediately. If the completed
        // generation never exposed a boundary, guard one boundary until the new
        // generation produces evidence of progress.
        ServerEvent::InputTranscript(text)
            if !text.trim().is_empty()
                && (state.input_transcript.has_fresh_epoch()
                    || !state.input_transcript.is_open()) =>
        {
            let dropped_events = state.terminal_dropped_events;
            let effectful_dropped_events = state.terminal_effectful_dropped_events;
            let stale_boundary_guarded = super::reader_policy::retire_terminal_for_user_turn(state);
            state.terminal_dropped_events = 0;
            state.terminal_effectful_dropped_events = 0;
            telemetry::event(
                "closed_generation_latch_retired",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "reason": "new_user_turn",
                    "stale_boundary_guarded": stale_boundary_guarded,
                    "dropped_events": dropped_events,
                    "effectful_dropped_events": effectful_dropped_events,
                }),
            );
            false
        }
        ServerEvent::InputTranscript(text) if !text.trim().is_empty() => {
            // Input transcription can trail the model boundary and revise the
            // utterance that already created this turn. Without a fresh speech
            // epoch, let the assembler update that turn but never revive it.
            telemetry::event(
                "late_input_transcript_correlated",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "char_count": text.chars().count(),
                    "terminal_response_open": state.terminal_response.is_open(),
                }),
            );
            false
        }
        ServerEvent::InputTranscript(_) => {
            count_drop(state, "empty_input_transcript", false);
            true
        }
        ServerEvent::Audio(samples)
            if state.terminal_response.accepts_output() && !samples.is_empty() =>
        {
            mark_response_started(state);
            false
        }
        ServerEvent::OutputTranscript(text)
            if state.terminal_response.accepts_output() && !text.trim().is_empty() =>
        {
            mark_response_started(state);
            false
        }
        ServerEvent::GenerationComplete => {
            handle_generation_complete(state, sink);
            true
        }
        ServerEvent::TurnComplete => {
            handle_boundary(state, sink);
            true
        }
        ServerEvent::Interrupted => {
            state.input_transcript.begin_epoch();
            super::speech_events::interrupted(state, sink);
            // Interrupted is progress metadata, not the old generation's final
            // boundary. A coalesced new transcript must still guard the late
            // TurnComplete that can follow this interruption.
            close_final_response(state, "interrupted", false, false);
            true
        }
        ServerEvent::ToolCall { id, name, .. } => {
            count_drop(state, "tool_call", true);
            state.immediate_tool_responses.push_back((
                id.clone(),
                name.clone(),
                serde_json::json!({
                    "ok": false,
                    "status": "blocked_terminal_generation",
                    "executed": false,
                    "error": {
                        "code": "turn_already_completed",
                        "message": "The prior generation is closed. Wait for a new user turn."
                    }
                }),
            ));
            close_final_response(state, "tool_call_rejected", false, false);
            true
        }
        ServerEvent::Audio(_)
        | ServerEvent::OutputTranscript(_)
        | ServerEvent::Thought(_)
        | ServerEvent::ModelText(_)
        | ServerEvent::ToolCancellation(_) => {
            let (kind, effectful) = dropped_event_class(event);
            count_drop(state, kind, effectful);
            true
        }
        // Connection metadata still belongs to the transport and remains live.
        ServerEvent::SetupComplete
        | ServerEvent::GoAway { .. }
        | ServerEvent::SessionResumption { .. }
        | ServerEvent::Usage(_)
        | ServerEvent::Other(_) => false,
    }
}

fn mark_response_started(state: &mut Reader) {
    state.terminal_activity_at = Some(std::time::Instant::now());
    state.think_start = None;
    if state.terminal_response != FinalResponseState::Streaming {
        state.terminal_response = FinalResponseState::Streaming;
        telemetry::event(
            "terminal_final_response_started",
            "runtime",
            Privacy::Safe,
            serde_json::json!({"accepted": state.terminal_accepted}),
        );
    }
}

fn handle_generation_complete(state: &mut Reader, sink: Option<&AudioSink>) {
    super::speech_events::generation_complete(state, sink);
    match state.terminal_response {
        FinalResponseState::AwaitingPriorBoundary => {
            let now = std::time::Instant::now();
            state.terminal_response = FinalResponseState::AwaitingOutput;
            state.terminal_prior_turn_boundary_pending = true;
            state.think_start = Some(now);
            state.terminal_activity_at = Some(now);
            telemetry::event(
                "terminal_prior_generation_complete",
                "runtime",
                Privacy::Safe,
                serde_json::json!({"accepted": state.terminal_accepted}),
            );
        }
        FinalResponseState::AwaitingOutput | FinalResponseState::Streaming => {
            if state.terminal_response == FinalResponseState::Streaming {
                state.terminal_prior_turn_boundary_pending = false;
            }
            state.terminal_generation_complete = true;
            state.terminal_activity_at = Some(std::time::Instant::now());
            state.terminal_final_response_delivered =
                state.terminal_response == FinalResponseState::Streaming;
            telemetry::event(
                "terminal_final_generation_complete",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "accepted": state.terminal_accepted,
                    "output_seen": state.terminal_final_response_delivered,
                    "awaiting_turn_boundary": true,
                }),
            );
        }
        FinalResponseState::Closed => {
            telemetry::event(
                "closed_generation_complete_observed",
                "runtime",
                Privacy::Safe,
                serde_json::json!({"dropped_events": state.terminal_dropped_events}),
            );
        }
    }
}

fn handle_boundary(state: &mut Reader, sink: Option<&AudioSink>) {
    super::speech_events::turn_complete(state, sink);
    if std::mem::take(&mut state.terminal_prior_turn_boundary_pending) {
        let now = std::time::Instant::now();
        if state.terminal_response == FinalResponseState::AwaitingOutput {
            state.think_start = Some(now);
        }
        state.terminal_activity_at = Some(now);
        telemetry::event(
            "terminal_prior_turn_boundary_observed",
            "runtime",
            Privacy::Safe,
            serde_json::json!({"accepted": state.terminal_accepted}),
        );
        return;
    }
    match state.terminal_response {
        FinalResponseState::AwaitingPriorBoundary => {
            let now = std::time::Instant::now();
            state.terminal_response = FinalResponseState::AwaitingOutput;
            state.think_start = Some(now);
            state.terminal_activity_at = Some(now);
            telemetry::event(
                "terminal_prior_boundary_observed",
                "runtime",
                Privacy::Safe,
                serde_json::json!({"accepted": state.terminal_accepted}),
            );
        }
        FinalResponseState::AwaitingOutput => {
            close_final_response(state, "turn_complete", true, false);
        }
        FinalResponseState::Streaming => {
            close_final_response(state, "turn_complete", true, true);
        }
        FinalResponseState::Closed => {
            observe_boundary(state, "turn_complete");
            overlay::set_status(if state.terminal_accepted {
                "ready - speak a command"
            } else {
                "blocked - speak a new command"
            });
        }
    }
}

fn close_final_response(
    state: &mut Reader,
    reason: &str,
    turn_boundary_seen: bool,
    response_completed: bool,
) {
    if !state.terminal_response.is_open() {
        if turn_boundary_seen {
            observe_boundary(state, reason);
        }
        return;
    }
    state.terminal_response = FinalResponseState::Closed;
    state.terminal_prior_turn_boundary_pending = false;
    state.active = false;
    state.awaiting = false;
    state.recovery_owed = false;
    state.think_start = None;
    state.terminal_activity_at = None;
    state.terminal_playback_cursor = None;
    flush_reply(state);
    if turn_boundary_seen {
        state.terminal_boundary_seen = true;
    }
    state.terminal_final_response_delivered = response_completed;
    telemetry::event(
        "terminal_final_response_closed",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "accepted": state.terminal_accepted,
            "reason": reason,
            "generation_complete_seen": state.terminal_generation_complete,
            "turn_boundary_seen": turn_boundary_seen,
            "response_completed": response_completed,
            "dropped_events": state.terminal_dropped_events,
            "effectful_dropped_events": state.terminal_effectful_dropped_events,
        }),
    );
    overlay::set_status(if state.terminal_accepted {
        "ready - speak a command"
    } else {
        "blocked - speak a new command"
    });
}

fn dropped_event_class(event: &ServerEvent) -> (&'static str, bool) {
    match event {
        ServerEvent::Audio(samples) => ("audio", !samples.is_empty()),
        ServerEvent::OutputTranscript(text) => ("output_transcript", !text.trim().is_empty()),
        ServerEvent::Thought(_) => ("thought", false),
        ServerEvent::ModelText(_) => ("model_text", false),
        ServerEvent::ToolCancellation(_) => ("tool_cancellation", false),
        _ => ("other", false),
    }
}

fn count_drop(state: &mut Reader, kind: &str, effectful: bool) {
    state.terminal_dropped_events = state.terminal_dropped_events.saturating_add(1);
    if effectful {
        state.terminal_effectful_dropped_events =
            state.terminal_effectful_dropped_events.saturating_add(1);
    }
    telemetry::event(
        "terminal_event_dropped",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "kind": kind,
            "effectful": effectful,
            "dropped_events": state.terminal_dropped_events,
            "effectful_dropped_events": state.terminal_effectful_dropped_events,
        }),
    );
}

fn observe_boundary(state: &mut Reader, boundary: &str) {
    state.terminal_boundary_seen = true;
    flush_reply(state);
    telemetry::event(
        "closed_generation_boundary_observed",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "boundary": boundary,
            "dropped_events": state.terminal_dropped_events,
        }),
    );
}

/// A replaced transport cannot deliver the rest of a terminal generation.
/// Retire it to idle instead of reseeding a synthetic continuation that could
/// repeat the final response.
pub(super) fn retire_for_connection_replacement(state: &mut Reader) -> bool {
    if !state.terminal_drain {
        return false;
    }
    if state.terminal_accepted && state.terminal_final_response_delivered {
        state.terminal_response = FinalResponseState::Closed;
        state.terminal_prior_turn_boundary_pending = false;
        state.active = false;
        state.awaiting = false;
        state.recovery_owed = false;
        state.think_start = None;
        state.terminal_activity_at = None;
        state.terminal_playback_cursor = None;
        telemetry::event(
            "terminal_completion_preserved",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "reason": "connection_replaced_after_final_delivery",
                "accepted": true,
            }),
        );
        return false;
    }
    state.terminal_drain = false;
    state.terminal_accepted = false;
    state.terminal_boundary_seen = false;
    state.terminal_dropped_events = 0;
    state.terminal_effectful_dropped_events = 0;
    state.terminal_response = FinalResponseState::Closed;
    state.terminal_generation_complete = false;
    state.terminal_prior_turn_boundary_pending = false;
    state.active = false;
    state.awaiting = false;
    state.recovery_owed = false;
    state.think_start = None;
    state.terminal_activity_at = None;
    state.terminal_playback_cursor = None;
    true
}

#[cfg(test)]
#[path = "terminal_drain/core_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "terminal_drain/boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
#[path = "terminal_drain/completion_tests.rs"]
mod completion_tests;

#[cfg(test)]
#[path = "terminal_drain/transcript_tests.rs"]
mod transcript_tests;
