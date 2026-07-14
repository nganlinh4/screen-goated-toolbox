//! Drain model-originated events after the current generation is closed.

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::protocol::ServerEvent;
use super::super::telemetry::{self, Privacy};
use super::reader::{Reader, flush_reply};

const FINAL_RESPONSE_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

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

pub(super) fn handle(event: &ServerEvent, sink: Option<&AudioSink>, state: &mut Reader) -> bool {
    if !state.terminal_drain {
        return false;
    }
    match event {
        // A real user turn owns the session immediately. If the completed
        // generation never exposed a boundary, guard one boundary until the new
        // generation produces evidence of progress.
        ServerEvent::InputTranscript(text) if !text.trim().is_empty() => {
            let dropped_events = state.terminal_dropped_events;
            let stale_boundary_guarded = super::reader_policy::retire_terminal_for_user_turn(state);
            state.terminal_dropped_events = 0;
            telemetry::event(
                "closed_generation_latch_retired",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "reason": "new_user_turn",
                    "stale_boundary_guarded": stale_boundary_guarded,
                    "dropped_events": dropped_events,
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
        ServerEvent::TurnComplete => {
            handle_boundary(state, sink);
            true
        }
        ServerEvent::Interrupted => {
            super::speech_events::interrupted(state, sink);
            // Interrupted is progress metadata, not the old generation's final
            // boundary. A coalesced new transcript must still guard the late
            // TurnComplete that can follow this interruption.
            close_final_response(state, sink, "interrupted", false);
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
            close_final_response(state, sink, "tool_call_rejected", false);
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

fn handle_boundary(state: &mut Reader, sink: Option<&AudioSink>) {
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
        FinalResponseState::AwaitingOutput | FinalResponseState::Streaming => {
            close_final_response(state, sink, "turn_complete", true);
        }
        FinalResponseState::Closed => {
            observe_boundary(state, "turn_complete");
            overlay::set_status("ready - speak a command");
        }
    }
}

/// Call only after a nonblocking socket read reports that its inbound queue is
/// empty. This ordering lets an already-queued final fragment refresh activity
/// before the deadline can close and suppress it.
pub(super) fn expire_after_socket_drained(state: &mut Reader, sink: Option<&AudioSink>) {
    if !state.terminal_response.is_open()
        || state
            .terminal_activity_at
            .is_none_or(|activity| activity.elapsed() < FINAL_RESPONSE_IDLE_TIMEOUT)
    {
        return;
    }
    let (code, message, reason) = match state.terminal_response {
        FinalResponseState::Streaming => (
            "ERR_TERMINAL_FINAL_BOUNDARY_MISSING",
            "terminal final response stopped streaming without a closing boundary",
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
    close_final_response(state, sink, reason, false);
}

fn close_final_response(
    state: &mut Reader,
    sink: Option<&AudioSink>,
    reason: &str,
    boundary_seen: bool,
) {
    if !state.terminal_response.is_open() {
        if boundary_seen {
            observe_boundary(state, reason);
        }
        return;
    }
    super::speech_events::generation_complete(state, sink);
    state.terminal_response = FinalResponseState::Closed;
    state.active = false;
    state.awaiting = false;
    state.recovery_owed = false;
    state.think_start = None;
    state.terminal_activity_at = None;
    flush_reply(state);
    if boundary_seen {
        state.terminal_boundary_seen = true;
    }
    telemetry::event(
        "terminal_final_response_closed",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "accepted": state.terminal_accepted,
            "reason": reason,
            "boundary_seen": boundary_seen,
            "dropped_events": state.terminal_dropped_events,
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
    telemetry::event(
        "terminal_event_dropped",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "kind": kind,
            "effectful": effectful,
            "dropped_events": state.terminal_dropped_events,
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
    state.terminal_drain = false;
    state.terminal_accepted = false;
    state.terminal_boundary_seen = false;
    state.terminal_dropped_events = 0;
    state.terminal_response = FinalResponseState::Closed;
    state.active = false;
    state.awaiting = false;
    state.recovery_owed = false;
    state.think_start = None;
    state.terminal_activity_at = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::protocol::parse_server_message;
    use serde_json::json;
    use std::sync::mpsc;

    #[test]
    fn connection_replacement_retires_an_open_final_response_to_idle() {
        let mut state = Reader::default();
        begin_final_response(&mut state, true);
        assert!(retire_for_connection_replacement(&mut state));
        assert!(!state.terminal_drain);
        assert!(!state.terminal_response.is_open());
        assert!(!state.active);
        assert!(!state.awaiting);
        assert!(!retire_for_connection_replacement(&mut state));
    }

    #[test]
    fn new_transcript_retires_latch_and_unseen_boundary_is_guarded() {
        let mut state = Reader {
            terminal_drain: true,
            ..Reader::default()
        };
        assert!(!handle(
            &ServerEvent::InputTranscript("new request".to_string()),
            None,
            &mut state
        ));
        assert!(!state.terminal_drain);
        assert!(state.ignore_stale_boundary);
    }

    #[test]
    fn empty_transcript_cannot_retire_a_closed_generation() {
        let mut state = Reader {
            terminal_drain: true,
            terminal_accepted: true,
            ..Reader::default()
        };
        assert!(handle(
            &ServerEvent::InputTranscript("  ".to_string()),
            None,
            &mut state
        ));
        assert!(state.terminal_drain);
        assert!(state.terminal_accepted);
        assert!(!state.ignore_stale_boundary);
    }

    #[test]
    fn ordinary_answer_boundary_latches_against_late_output_and_tools() {
        let mut state = Reader {
            active: true,
            awaiting: true,
            recovery_owed: true,
            ..Reader::default()
        };
        let (exec_tx, exec_rx) = mpsc::channel();
        super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
        assert!(state.terminal_drain);
        assert!(state.terminal_accepted);
        super::super::reader::handle_event(
            ServerEvent::OutputTranscript("late".to_string()),
            None,
            &exec_tx,
            &mut state,
        );
        super::super::reader::handle_event(
            ServerEvent::ToolCall {
                id: "late-call".to_string(),
                name: "future_tool".to_string(),
                args: json!({}),
            },
            None,
            &exec_tx,
            &mut state,
        );
        assert!(state.reply.is_empty());
        assert!(exec_rx.try_recv().is_err());
        assert_eq!(state.immediate_tool_responses.len(), 1);
    }

    #[test]
    fn unverified_action_boundary_is_closed_but_not_accepted() {
        let mut state = Reader {
            active: true,
            awaiting: true,
            turn_mode: crate::overlay::computer_control::turn_policy::TurnMode::Action,
            ..Reader::default()
        };
        let (exec_tx, _exec_rx) = mpsc::channel();
        super::super::reader::handle_event(ServerEvent::TurnComplete, None, &exec_tx, &mut state);
        assert!(state.terminal_drain);
        assert!(!state.terminal_accepted);
    }

    #[test]
    fn model_rationale_cannot_replace_the_committed_user_goal() {
        let mut state = Reader::default();
        let (exec_tx, exec_rx) = mpsc::channel();
        super::super::reader::handle_event(
            ServerEvent::InputTranscript("committed user goal".to_string()),
            None,
            &exec_tx,
            &mut state,
        );
        super::super::reader::handle_event(
            ServerEvent::Thought("different model rationale".to_string()),
            None,
            &exec_tx,
            &mut state,
        );
        super::super::reader::handle_event(
            ServerEvent::ToolCall {
                id: "call".to_string(),
                name: "future_tool".to_string(),
                args: json!({}),
            },
            None,
            &exec_tx,
            &mut state,
        );
        let job = exec_rx.try_recv().expect("tool job");
        assert_eq!(job.task, "committed user goal");
        assert_eq!(job.user_text, "committed user goal");
        assert_eq!(state.last_cmd, "committed user goal");
    }

    #[test]
    fn first_real_output_clears_only_recovery_debt() {
        let mut state = Reader {
            active: true,
            awaiting: true,
            recovery_owed: true,
            ..Reader::default()
        };
        let (exec_tx, _exec_rx) = mpsc::channel();
        super::super::reader::handle_event(
            ServerEvent::OutputTranscript("answer".to_string()),
            None,
            &exec_tx,
            &mut state,
        );
        assert!(state.active);
        assert!(state.awaiting);
        assert!(!state.recovery_owed);
        assert!(!super::super::reader_policy::recovery_due(&state));
    }

    #[test]
    fn done_with_coalesced_boundary_cannot_enable_late_tools() {
        let mut state = Reader::default();
        let (exec_tx, _exec_rx) = mpsc::channel();
        let raw = r#"{"serverContent":{"turnComplete":true},"toolCall":{"functionCalls":[{"id":"done-call","name":"done","args":{"summary":"complete"}}]}}"#;
        let events = parse_server_message(raw);
        super::super::reader_policy::begin_terminal_drain(&mut state, true, false);
        for event in events
            .into_iter()
            .filter(|event| matches!(event, ServerEvent::TurnComplete))
        {
            super::super::reader::handle_event(event, None, &exec_tx, &mut state);
        }
        assert!(state.terminal_drain);
        super::super::reader::handle_event(
            ServerEvent::ToolCall {
                id: "late-call".to_string(),
                name: "future_capability".to_string(),
                args: json!({}),
            },
            None,
            &exec_tx,
            &mut state,
        );
        assert!(state.terminal_drain);
        assert_eq!(state.immediate_tool_responses.len(), 1);
    }
}

#[cfg(test)]
#[path = "terminal_drain/boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
#[path = "terminal_drain/transcript_tests.rs"]
mod transcript_tests;
