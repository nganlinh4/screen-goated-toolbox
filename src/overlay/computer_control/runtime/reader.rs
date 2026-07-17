//! Reader state, rolling conversation history, and server-event handling.

use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Instant;

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::protocol::ServerEvent;
use super::super::telemetry::{self, Privacy};
use super::super::turn_policy;
use super::Job;
pub(super) use super::reader_state::{Pending, Reader};

/// Cap on history entries kept (rolling); older turns drop off. Sized to retain
/// a whole session for conversation MEMORY (the reconnect recap is bounded
/// separately by RECAP_BUDGET, so a larger window costs only a little RAM).
const MAX_HISTORY: usize = 600;

/// Close out the assistant's accumulated reply into the conversation history.
pub(super) fn flush_reply(state: &mut Reader) {
    let r = state.reply.trim();
    if !r.is_empty() {
        let clipped: String = r.chars().take(600).collect();
        let utterance_id = state
            .reply_utterance_id
            .take()
            .or(state.assistant_utterance_id)
            .unwrap_or_else(|| telemetry::next_utterance("assistant_reply_flushed"));
        telemetry::human(
            "cc",
            format!(
                "assistant transcript completed ({} chars)",
                state.reply.chars().count()
            ),
        );
        telemetry::event(
            "assistant_reply",
            "speech",
            Privacy::UserText,
            serde_json::json!({
                "utterance_id": utterance_id,
                "text_preview": clipped,
                "char_count": r.chars().count(),
            }),
        );
        state.history.push(format!("Assistant: {clipped}"));
        if state.history.len() > MAX_HISTORY {
            let drop = state.history.len() - MAX_HISTORY;
            state.history.drain(0..drop);
        }
    }
    state.reply.clear();
    state.reply_utterance_id = None;
}

/// Append a compact note of what a vision/read tool actually OBSERVED to the
/// rolling history, so it survives a reconnect (the preview session's own memory is
/// unreliable) and the agent can recall the sequence later — e.g. summarize all the
/// dialogue it read, not just whatever happens to still be on screen.
pub(super) fn record_observation(state: &mut Reader, name: &str, resp: &Value) {
    // In the live path the tool output is nested under "action_result"; look/read
    // tools put their text in "reading" and browser_read_page in page.text. Read
    // whichever is present (falling back to the bare resp for non-wrapped callers)
    // so the observation survives a reconnect via the recap - without this the
    // wrapper hid every reading and the reconnect memory recorded nothing.
    let inner = resp.get("action_result").unwrap_or(resp);
    let Some(reading) = inner.get("reading").and_then(Value::as_str).or_else(|| {
        inner
            .get("page")
            .and_then(|p| p.get("text"))
            .and_then(Value::as_str)
    }) else {
        return; // only look/read/browser_read_page carry text; actions don't
    };
    let reading = reading.trim();
    if reading.is_empty() {
        return;
    }
    let clipped: String = reading.chars().take(220).collect();
    state.history.push(format!("Observed ({name}): {clipped}"));
    if state.history.len() > MAX_HISTORY {
        let drop = state.history.len() - MAX_HISTORY;
        state.history.drain(0..drop);
    }
}

pub(super) fn record_tool_result(state: &mut Reader, name: &str) {
    if name == "research_web" {
        state.turn_research_count += 1;
    }
}

pub(super) fn emit_turn_summary(state: &mut Reader, outcome: &str) {
    if state.turn_summary_emitted {
        return;
    }
    let duration_ms = state
        .turn_started_at
        .map(|started| started.elapsed().as_millis())
        .unwrap_or(0);
    let class = turn_policy::task_class_from_tools(&state.turn_tools);
    telemetry::event(
        "turn_summary",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "outcome": outcome,
            "task_class": class,
            "turn_mode": state.turn_mode.as_str(),
            "duration_ms": duration_ms,
            "tool_count": state.turn_tools.len(),
            "tools": state.turn_tools.clone(),
            "stall_count": state.turn_stall_count,
            "research_count": state.turn_research_count,
            "generation_count": state.model_generation_index,
            "usage_event_count": state.usage_event_index,
            "tool_response_bytes": state.turn_tool_response_bytes,
            "element_chars_delivered": state.turn_element_chars,
            "user_char_count": state.last_user_text.chars().count(),
            "goal_char_count": state.last_cmd.chars().count(),
        }),
    );
    state.turn_summary_emitted = true;
}

pub(super) fn handle_event(
    ev: ServerEvent,
    sink: Option<&AudioSink>,
    exec_tx: &mpsc::Sender<Job>,
    state: &mut Reader,
) {
    if super::terminal_drain::handle(&ev, sink, state) {
        return;
    }
    if super::reader_policy::is_real_generation_progress(&ev) {
        super::reader_policy::record_generation_progress(state);
    }
    match ev {
        ServerEvent::Audio(pcm) => super::speech_events::audio(state, &pcm, sink),
        ServerEvent::Interrupted => {
            // Barge-in: stop TALKING so the agent listens, but let the in-flight
            // ACTION finish until the server identifies which pending call its
            // voice-activity barge-in cancelled.
            state.input_transcript.begin_epoch();
            super::speech_events::interrupted(state, sink);
        }
        ServerEvent::ToolCancellation(ids) => {
            // A server-cancelled id must not receive a response; abort its in-flight
            // action because the model is about to re-plan from the new input. This is
            // the server's voice-activity barge-in, which works in ANY language, so
            // language-independent and needs no spoken-keyword list.
            super::speech_events::interrupted(state, sink);
            let owns_pending = state
                .pending
                .id
                .as_ref()
                .is_some_and(|pending| ids.iter().any(|id| id == pending));
            if owns_pending {
                state.pending.request_cancel(); // don't answer it; abort only that job
                super::reader_policy::mark_pending_interruption(state);
                state.awaiting = true;
                state.recovery_owed = true;
                state.think_start = Some(Instant::now());
                overlay::set_status("halting...");
            }
            overlay::push_log(format!(
                "[~] cancellation requested; the action may already have taken effect {ids:?}"
            ));
            telemetry::typed_error(
                "ERR_TOOL_CANCELLED",
                "runtime",
                "server cancelled a pending tool call after barge-in",
                serde_json::json!({"ids": ids, "pending_cancelled": state.pending.cancelled}),
            );
        }
        ServerEvent::InputTranscript(t) => {
            if let Some(update) = state.input_transcript.merge(&t) {
                let text = update.text;
                if update.starts_turn {
                    let inherit_goal = state.carry_unfinished_goal
                        || (state.turn_mode == turn_policy::TurnMode::Action
                            && (state.active || state.awaiting || state.pending.id.is_some()));
                    state.active_goal.start_turn(&text, inherit_goal);
                    state.carry_unfinished_goal = false;
                    super::speech_events::interrupted(state, sink);
                    if state.active && !state.turn_summary_emitted {
                        emit_turn_summary(state, "superseded");
                    }
                    flush_reply(state);
                    telemetry::start_turn("user_transcript");
                    telemetry::human(
                        "cc",
                        format!("user transcript received ({} chars)", text.chars().count()),
                    );
                    telemetry::event(
                        "user_transcript",
                        "speech",
                        Privacy::UserText,
                        serde_json::json!({
                            "text_preview": text.chars().take(240).collect::<String>(),
                            "char_count": text.chars().count(),
                            "fragment_index": update.fragment_index,
                            "starts_turn": update.starts_turn,
                            "changed": update.changed,
                            "finality": "provider_unspecified",
                        }),
                    );
                    state.assistant_utterance_id = None;
                    state.reasoning.clear();
                    state.thinking.clear();
                    state.turn_started_at = Some(Instant::now());
                    state.turn_tools.clear();
                    state.turn_outcomes.clear();
                    state.turn_research_count = 0;
                    state.model_generation_index = 1;
                    state.usage_event_index = 0;
                    state.turn_tool_response_bytes = 0;
                    state.turn_element_chars = 0;
                    state.turn_stall_count = 0;
                    state.turn_summary_emitted = false;
                    state.generation_output_seen = false;
                    state.pending_tool_output_seen = false;
                    state.terminal_final_response_delivered = false;
                    state.history.push(format!("User: {text}"));
                    if state.history.len() > MAX_HISTORY {
                        state.history.remove(0);
                    }
                    let cancelled_pending =
                        super::reader_policy::apply_user_turn_policy(state, &text);
                    telemetry::event(
                        "turn_policy_applied",
                        "turn_policy",
                        Privacy::Safe,
                        serde_json::json!({
                            "turn_mode": state.turn_mode.as_str(),
                            "cancelled_pending": cancelled_pending,
                        }),
                    );
                } else if update.changed {
                    state.active_goal.update_current(&text);
                    if let Some(entry) = state
                        .history
                        .iter_mut()
                        .rfind(|entry| entry.starts_with("User:"))
                    {
                        *entry = format!("User: {text}");
                    }
                    telemetry::event(
                        "user_transcript_updated",
                        "speech",
                        Privacy::UserText,
                        serde_json::json!({
                            "text_preview": text.chars().take(240).collect::<String>(),
                            "char_count": text.chars().count(),
                            "fragment_char_count": t.trim().chars().count(),
                            "fragment_index": update.fragment_index,
                            "starts_turn": update.starts_turn,
                            "finality": "provider_unspecified",
                        }),
                    );
                }
                state.last_cmd = state.active_goal.render();
                state.last_user_text.clone_from(&text);
                overlay::set_user_text(text);
            }
            overlay::set_listening(false);
        }
        ServerEvent::OutputTranscript(t) => super::speech_events::transcript(state, &t, sink),
        ServerEvent::ModelText(_) => {
            // modelTurn text parts in AUDIO mode carry tool-call / internal text
            // (e.g. "call:look{...}"), NOT spoken words — ignore so they don't
            // pollute the spoken transcript or the vision intent context.
        }
        ServerEvent::Thought(t) => {
            // The model's SILENT reasoning (thinking) — never spoken, never shown. Feed it to
            // the intent buffer so the turn's task is captured even on a wordless turn.
            state.thinking.push_str(&t);
        }
        ServerEvent::GenerationComplete => {
            super::speech_events::generation_complete(state, sink);
        }
        ServerEvent::TurnComplete => {
            if super::reader_policy::consume_stale_boundary(state) {
                telemetry::event(
                    "stale_generation_boundary_ignored",
                    "runtime",
                    Privacy::Safe,
                    serde_json::json!({"active_newer_turn": state.active}),
                );
                return;
            }
            // The server's boundary ends this response. Never turn it into a
            // synthetic "continue" request: that creates unsolicited speech and
            // can loop forever when the model does not call done explicitly.
            let generation_output_seen = state.generation_output_seen;
            let boundary = super::reader_policy::finish_at_model_boundary(state);
            if boundary == super::reader_policy::BoundaryOutcome::PendingTool {
                state.pending_tool_boundary_seen = true;
            }
            match boundary {
                super::reader_policy::BoundaryOutcome::ConversationComplete
                | super::reader_policy::BoundaryOutcome::ActionComplete => {
                    super::speech_events::release_generation_audio(
                        state,
                        sink,
                        "model_turn_boundary",
                    );
                    emit_turn_summary(state, "model_turn_complete");
                    overlay::set_orb_done();
                    overlay::set_status("ready - speak a command");
                }
                super::reader_policy::BoundaryOutcome::PendingTool
                | super::reader_policy::BoundaryOutcome::AlreadyIdle => {}
            }
            super::speech_events::turn_complete(state, sink);
            if matches!(
                boundary,
                super::reader_policy::BoundaryOutcome::ConversationComplete
                    | super::reader_policy::BoundaryOutcome::ActionComplete
            ) {
                flush_reply(state);
            }
            state.awaiting = false;
            state.recovery_owed = false;
            match boundary {
                super::reader_policy::BoundaryOutcome::ConversationComplete
                | super::reader_policy::BoundaryOutcome::ActionComplete => {
                    state.terminal_final_response_delivered = generation_output_seen;
                    super::reader_policy::begin_terminal_drain(state, true, true);
                    telemetry::event(
                        "model_generation_closed",
                        "runtime",
                        Privacy::Safe,
                        serde_json::json!({
                            "accepted": true,
                            "reason": "model_turn_complete",
                            "generation_complete_seen": true,
                            "turn_boundary_seen": true,
                            "response_completed": generation_output_seen,
                            "dropped_events": 0,
                            "effectful_dropped_events": 0,
                        }),
                    );
                    let cleanup_turn_id = telemetry::current_turn();
                    let cleanup = Job {
                        id: super::RETIRE_TURN.to_string(),
                        name: super::RETIRE_TURN.to_string(),
                        args: serde_json::json!({}),
                        task: String::new(),
                        user_text: String::new(),
                        inherit_evidence: false,
                        action: telemetry::ActionTrace {
                            action_id: 0,
                            turn_id: cleanup_turn_id,
                        },
                        source_frame: None,
                        queued_at: Instant::now(),
                        cancel: Arc::new(AtomicBool::new(false)),
                    };
                    if exec_tx.send(cleanup).is_err() {
                        telemetry::typed_error(
                            "ERR_TURN_RETIRE_ENQUEUE",
                            "runtime",
                            "could not enqueue local turn cleanup",
                            serde_json::json!({}),
                        );
                    } else {
                        state.turn_cleanup_pending = Some(cleanup_turn_id);
                        telemetry::event(
                            "turn_cleanup_enqueued",
                            "runtime",
                            Privacy::Safe,
                            serde_json::json!({
                                "cleanup_turn_id": cleanup_turn_id,
                                "source": "model_turn_complete",
                            }),
                        );
                    }
                }
                _ => {}
            }
            state.generation_output_seen = false;
        }
        ServerEvent::ToolCall { id, name, args } => {
            let tool_generation_output_seen = state.generation_output_seen;
            if name != "done" {
                super::speech_events::release_generation_audio(state, sink, "progress_tool_call");
            }
            state.generation_output_seen = false;
            state.awaiting = false; // model responded (with an action)
            state.recovery_owed = false;
            if let Some(t) = state.think_start.take() {
                let ms = t.elapsed().as_millis();
                let spoke = !state.reasoning.trim().is_empty();
                if spoke {
                    let preview: String = state.reasoning.trim().chars().take(220).collect();
                    overlay::push_log(format!(
                        "[speech before {name}] {} chars",
                        state.reasoning.trim().chars().count()
                    ));
                    telemetry::event(
                        "speech_before_tool",
                        "runtime",
                        Privacy::UserText,
                        serde_json::json!({
                            "tool": name.clone(),
                            "char_count": state.reasoning.trim().chars().count(),
                            "preview": preview,
                        }),
                    );
                }
                overlay::push_log(format!(
                    "[think {ms}ms{}]",
                    if spoke { ", spoke" } else { "" }
                ));
                telemetry::event(
                    "think_complete",
                    "runtime",
                    Privacy::Safe,
                    serde_json::json!({"tool": name.clone(), "duration_ms": ms, "spoke": spoke}),
                );
                state.tool_calls += 1;
                state.think_total_ms += ms;
                state.spoke_count += u32::from(spoke);
                if state.tool_calls.is_multiple_of(12) {
                    overlay::push_log(format!(
                        "[PROFILE] {} actions | avg think {}ms | {} spoke | {} stalls",
                        state.tool_calls,
                        state.think_total_ms / u128::from(state.tool_calls),
                        state.spoke_count,
                        state.stall_count,
                    ));
                }
            }
            // Intent = the model's SILENT thinking if present (preferred - it's the real
            // reasoning and costs no speech), else its spoken words. Capped so a long thought
            // summary can't bloat the vision context.
            let from_think = !state.thinking.trim().is_empty();
            let src = if from_think {
                state.thinking.trim()
            } else {
                state.reasoning.trim()
            };
            let intent: String = src.chars().take(500).collect();
            state.reasoning.clear();
            state.thinking.clear();
            if !intent.is_empty() {
                overlay::push_log(format!(
                    "[intent/{}] {} chars",
                    if from_think { "thought" } else { "spoken" },
                    intent.chars().count(),
                ));
            }
            super::reader_policy::refine_turn_mode(state, &intent, &name);
            let action = telemetry::next_step(&name);
            let step_id = action.action_id;
            state.turn_tools.push(name.clone());
            let args_metadata = telemetry::value_metadata(&args);
            overlay::push_log(format!(">{name} args={} bytes", args.to_string().len()));
            telemetry::event(
                "tool_call",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "step_id": step_id,
                    "tool_call_id": id.clone(),
                    "name": name.clone(),
                    "args_metadata": args_metadata,
                    "goal_char_count": state.last_cmd.chars().count(),
                    "intent_char_count": intent.chars().count(),
                    "turn_mode": state.turn_mode.as_str(),
                    "source_frame_id": state.source_frame.as_ref().map(|frame| frame.frame_id),
                    "source_surface": state.source_frame.as_ref().map(|frame| &frame.surface),
                }),
            );
            telemetry::event(
                "tool_call_payload",
                "runtime",
                Privacy::Sensitive,
                serde_json::json!({
                    "step_id": step_id,
                    "tool_call_id": id.clone(),
                    "name": name.clone(),
                    "args": args.clone(),
                }),
            );

            if super::reader_policy::guard_tool_call(state, &id, &name, &args, action) {
                return;
            }
            state.pending_tool_output_seen = tool_generation_output_seen;

            overlay::set_status(format!("doing: {name}"));
            overlay::set_orb_tool(&name, &args);
            let cancel = Arc::new(AtomicBool::new(false));
            state.pending = Pending {
                id: Some(id.clone()),
                tool: Some(name.clone()),
                turn_id: Some(action.turn_id),
                cancelled: false,
                cancel: Some(cancel.clone()),
            };
            state.pending_tool_boundary_seen = false;
            // Runs on the executor thread (the Brain dispatch + grounding).
            let job = Job {
                id: id.clone(),
                name: name.clone(),
                args,
                task: state.last_cmd.clone(),
                user_text: state.last_user_text.clone(),
                inherit_evidence: state.active_goal.is_continuation(),
                action,
                source_frame: state.source_frame.clone(),
                queued_at: Instant::now(),
                cancel,
            };
            if exec_tx.send(job).is_err() {
                state.pending.request_cancel();
                state.pending = Pending::default();
                state.pending_tool_output_seen = false;
                state.immediate_tool_responses.push_back((
                    id.clone(),
                    name.clone(),
                    serde_json::json!({
                        "ok": false,
                        "error": {
                            "code": "executor_unavailable",
                            "message": "The local action worker is unavailable; no action was performed."
                        }
                    }),
                ));
                state.awaiting = true;
                state.recovery_owed = true;
                state.think_start = Some(Instant::now());
                telemetry::typed_error(
                    "ERR_ACTION_EXECUTOR_UNAVAILABLE",
                    "runtime",
                    "failed to enqueue a tool call because the action worker stopped",
                    serde_json::json!({"tool": name}),
                );
                telemetry::event_for_action(
                    "action_outcome",
                    "runtime",
                    Privacy::Safe,
                    action,
                    serde_json::json!({
                        "tool_call_id": id,
                        "requested_tool": name,
                        "executed": false,
                        "status": "executor_unavailable",
                        "ok": false,
                    }),
                );
            }
        }
        ServerEvent::GoAway { time_left } => {
            overlay::push_log(format!(
                "server goAway ({time_left}) - reconnect queued after turn and playback"
            ));
            state.go_away = true;
        }
        ServerEvent::Usage(usage) => {
            state.usage_event_index = state.usage_event_index.saturating_add(1);
            telemetry::event(
                "model_usage",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "usage": usage,
                    "usage_event_index": state.usage_event_index,
                    "generation_index": state.model_generation_index,
                    "tool_cycle_index": state.model_generation_index.saturating_sub(1),
                }),
            );
        }
        _ => {}
    }
}
