//! Reader state, rolling conversation history, and server-event handling.

use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::protocol::ServerEvent;
use super::super::telemetry::{self, Privacy};
use super::super::turn_policy;
use super::Job;
use super::speech_gate::SpeechGate;

/// The single in-flight tool call (synchronous FC ⇒ at most one), plus whether the
/// server cancelled it (in which case we must NOT answer it).
#[derive(Default)]
pub(super) struct Pending {
    pub(super) id: Option<String>,
    pub(super) cancelled: bool,
    pub(super) cancel: Option<Arc<AtomicBool>>,
}

impl Pending {
    /// Cancel only this job. The token is never reused by a later action.
    pub(super) fn request_cancel(&mut self) -> bool {
        let had_job = self.id.is_some();
        if had_job {
            self.cancelled = true;
            if let Some(cancel) = &self.cancel {
                cancel.store(true, Ordering::SeqCst);
            }
        }
        had_job
    }

    pub(super) fn matches_result(&self, id: &str, cancel: &Arc<AtomicBool>) -> bool {
        self.id.as_deref() == Some(id)
            && self
                .cancel
                .as_ref()
                .is_some_and(|pending_cancel| Arc::ptr_eq(pending_cancel, cancel))
    }
}

/// Mutable reader-side session state threaded through `handle_event`.
#[derive(Default)]
pub(super) struct Reader {
    pub(super) pending: Pending,
    /// The model's spoken output since the last tool call - its "intent" context.
    pub(super) reasoning: String,
    /// The model's SILENT thinking (includeThoughts) since the last tool call - the
    /// preferred intent source: captured even when the model says nothing aloud.
    pub(super) thinking: String,
    /// The MODEL's own understanding of the current task (its first stated intent this
    /// turn) - the goal handed to vision + done-verification. NOT the raw input
    /// transcription: that text is display-only, so a mis-hear can't pollute the task.
    pub(super) last_cmd: String,
    /// Raw user transcript for the current turn, retained for model context and
    /// completion evidence but never parsed to grant or deny tools.
    pub(super) last_user_text: String,
    /// When the current user turn started, for compact turn summaries.
    pub(super) turn_started_at: Option<Instant>,
    pub(super) turn_tools: Vec<String>,
    pub(super) turn_research_count: u32,
    pub(super) turn_stall_count: u32,
    pub(super) turn_summary_emitted: bool,
    /// Authorization/lifecycle mode for the current user turn.
    pub(super) turn_mode: turn_policy::TurnMode,
    /// Policy-owned responses waiting for the socket loop to send.
    pub(super) immediate_tool_responses: VecDeque<super::reader_policy::ImmediateToolResponse>,
    /// Control nudge to send after the current server frame is fully processed.
    pub(super) control_nudge: Option<String>,
    /// Exact latest frame successfully sent to the Live model.
    pub(super) source_frame_id: Option<u64>,
    pub(super) assistant_utterance_id: Option<u64>,
    pub(super) connection_generation: u32,
    pub(super) reconnect_total: u32,
    /// An accepted `done` still receives one final server `TurnComplete`. Do not
    /// let that old boundary close a newer user turn.
    pub(super) awaiting_done_boundary: bool,
    /// True once the user has issued any command this session - gates the one-time
    /// browser-control offer (replaces using `last_cmd` non-empty for that, since
    /// `last_cmd` now tracks the model's intent, not "did the user speak").
    pub(super) has_command: bool,
    /// True while a spoken request is being worked on. Idle frames are pushed only
    /// while active, so after `done` the agent waits for the user instead of
    /// treating each new frame as a cue to keep acting.
    pub(super) active: bool,
    /// Rolling conversation history (alternating "User:"/"Assistant:" lines). The
    /// preview model rejects sessionResumption, so on a dropped connection we
    /// re-seed a fresh session with this recap - the agent keeps its memory.
    pub(super) history: Vec<String>,
    /// The assistant's spoken reply since the last user turn, flushed into
    /// `history` when the user speaks again (or on reconnect).
    pub(super) reply: String,
    /// True when the model OWES us a response (user just spoke, or we just answered
    /// a tool call) and hasn't produced output yet. The staleness heartbeat fires
    /// only while awaiting — so a normal idle wait for the user never reconnects.
    pub(super) awaiting: bool,
    /// Set once we've nudged the model during the CURRENT silent spell, so we poke
    /// it only once before escalating to a reconnect. Cleared on any server event.
    pub(super) nudged: bool,
    /// The server sent a `goAway` (session is hitting its duration limit). The run
    /// loop reconnects PROACTIVELY at the next gap so we migrate cleanly with our
    /// recap, instead of being force-closed mid-stream.
    pub(super) go_away: bool,
    /// When the model started OWING us a response (user spoke, or we answered a tool
    /// call). Used to log model THINK-time and to catch turns that end mid-task with
    /// no action ("narrated but didn't act").
    pub(super) think_start: Option<Instant>,
    // Rolling diagnostics, logged as a [PROFILE] line every 12 actions.
    pub(super) tool_calls: u32,
    pub(super) think_total_ms: u128,
    pub(super) spoke_count: u32,
    pub(super) stall_count: u32,
    pub(super) speech_gate: SpeechGate,
}

/// Cap on history entries kept (rolling); older turns drop off. Sized to retain
/// a whole session for conversation MEMORY (the reconnect recap is bounded
/// separately by RECAP_BUDGET, so a larger window costs only a little RAM).
const MAX_HISTORY: usize = 600;
/// Cap on the recap text seeded on reconnect. Kept modest (~600 tokens) so the seed message
/// stays small — a giant text seed risks the server's "invalid argument" close and would just get
/// sliding-window-compressed away anyway. A frame is re-seeded alongside this for the visual state.
const RECAP_BUDGET: usize = 2400;

/// Close out the assistant's accumulated reply into the conversation history.
pub(super) fn flush_reply(state: &mut Reader) {
    let r = state.reply.trim();
    if !r.is_empty() {
        let clipped: String = r.chars().take(600).collect();
        let utterance_id = state
            .assistant_utterance_id
            .unwrap_or_else(|| telemetry::next_utterance("assistant_reply_flushed"));
        telemetry::human(
            "cc",
            format!(
                "generated transcript ({} chars): {clipped}",
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

pub(super) fn record_tool_result(state: &mut Reader, name: &str, resp: &Value) {
    if name == "research_web"
        || resp
            .pointer("/action_result/rerouted_from")
            .is_some_and(|v| !v.is_null())
    {
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
            "user_preview": state.last_user_text.chars().take(180).collect::<String>(),
            "intent_preview": state.last_cmd.chars().take(180).collect::<String>(),
        }),
    );
    state.turn_summary_emitted = true;
}

/// Build a recap of the most recent conversation (newest-biased, length-capped).
pub(super) fn build_recap(history: &[String]) -> String {
    let mut picked: Vec<&str> = Vec::new();
    let mut total = 0;
    for line in history.iter().rev() {
        if total + line.len() > RECAP_BUDGET {
            break;
        }
        total += line.len();
        picked.push(line);
    }
    picked.reverse();
    picked.join("\n")
}

pub(super) fn handle_event(
    ev: ServerEvent,
    sink: Option<&AudioSink>,
    exec_tx: &mpsc::Sender<Job>,
    state: &mut Reader,
) {
    match ev {
        ServerEvent::Audio(pcm) => super::speech_events::audio(state, &pcm, sink),
        ServerEvent::Interrupted => {
            // Barge-in: stop TALKING so the agent listens, but let the in-flight
            // ACTION finish until the server identifies which pending call its
            // voice-activity barge-in cancelled.
            super::speech_events::interrupted(state, sink);
        }
        ServerEvent::ToolCancellation(ids) => {
            // A server-cancelled id must not receive a response; abort its in-flight
            // action because the model is about to re-plan from the new input. This is
            // the server's voice-activity barge-in, which works in ANY language, so
            // language-independent and needs no spoken-keyword list.
            super::speech_events::interrupted(state, sink);
            if let Some(p) = state.pending.id.as_ref()
                && ids.iter().any(|i| i == p)
            {
                state.pending.request_cancel(); // don't answer it; abort only that job
                state.awaiting = true;
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
            // ASR can mis-hear, so this stays out of task/vision context; the model's
            // first intent still supplies that. The transcript is authoritative only
            // for explicit safety restrictions and code-owned capability policy.
            if !t.trim().is_empty() {
                if state.active && !state.turn_summary_emitted {
                    emit_turn_summary(state, "superseded");
                }
                flush_reply(state); // close the assistant's prior reply into history
                telemetry::start_turn("user_transcript");
                telemetry::human("cc", format!("you: {}", t.trim()));
                telemetry::event(
                    "user_transcript",
                    "speech",
                    Privacy::UserText,
                    serde_json::json!({
                        "text_preview": t.trim().chars().take(240).collect::<String>(),
                        "char_count": t.trim().chars().count(),
                    }),
                );
                state.assistant_utterance_id = None;
                state.reasoning.clear(); // this turn's intent starts fresh - don't inherit last turn's narration
                state.thinking.clear();
                state.speech_gate.reset();
                state.last_cmd.clear(); // new turn - the goal is re-derived from the model's intent
                state.last_user_text = t.trim().to_string();
                state.turn_started_at = Some(Instant::now());
                state.turn_tools.clear();
                state.turn_research_count = 0;
                state.turn_stall_count = 0;
                state.turn_summary_emitted = false;
                state.has_command = true; // the user has spoken (gates the browser offer)
                let cancelled_pending =
                    super::reader_policy::apply_user_turn_policy(state, t.trim());
                telemetry::event(
                    "turn_policy_applied",
                    "turn_policy",
                    Privacy::Safe,
                    serde_json::json!({
                        "turn_mode": state.turn_mode.as_str(),
                        "cancelled_pending": cancelled_pending,
                    }),
                );
            }
            overlay::set_user_text(t);
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
        ServerEvent::TurnComplete => {
            if state.awaiting_done_boundary {
                state.awaiting_done_boundary = false;
                super::speech_events::generation_complete(state, sink);
                flush_reply(state);
                if !state.active {
                    state.awaiting = false;
                }
                telemetry::event(
                    "done_turn_boundary",
                    "runtime",
                    Privacy::Safe,
                    serde_json::json!({"active_newer_turn": state.active}),
                );
                return;
            }
            // The server's boundary ends this response. Never turn it into a
            // synthetic "continue" request: that creates unsolicited speech and
            // can loop forever when the model does not call done explicitly.
            if super::reader_policy::finish_at_model_boundary(state) {
                emit_turn_summary(state, "model_turn_complete");
                overlay::set_orb_done();
                overlay::set_status("ready - speak a command");
            }
            super::speech_events::generation_complete(state, sink);
            flush_reply(state);
            state.awaiting = false;
        }
        ServerEvent::ToolCall { id, name, args } => {
            let pre_tool_speech_suppressed =
                super::speech_events::generation_before_tool(state, sink);
            state.awaiting = false; // model responded (with an action)
            if let Some(t) = state.think_start.take() {
                let ms = t.elapsed().as_millis();
                let spoke = !pre_tool_speech_suppressed && !state.reasoning.trim().is_empty();
                if spoke {
                    let preview: String = state.reasoning.trim().chars().take(220).collect();
                    overlay::push_log(format!(
                        "[speech before {name}] {} chars: {preview}",
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
            // The model's first stated intent this turn IS the task (it replaces the raw input
            // transcription as the goal for vision + done-verification). Log it once per turn with
            // its source so a test can confirm thought-capture (includeThoughts) is working.
            if state.last_cmd.is_empty() && !intent.is_empty() {
                state.last_cmd = intent.clone();
                let preview: String = intent.chars().take(80).collect();
                overlay::push_log(format!(
                    "[intent/{}] {preview}",
                    if from_think { "thought" } else { "spoken" }
                ));
            }
            super::reader_policy::refine_turn_mode(state, &intent, &name);
            let action = telemetry::next_step(&name);
            let step_id = action.action_id;
            state.turn_tools.push(name.clone());
            overlay::push_log(format!(">{name} {}", compact_args(&args)));
            telemetry::event(
                "tool_call",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "step_id": step_id,
                    "tool_call_id": id.clone(),
                    "name": name.clone(),
                    "args_preview": compact_args(&args),
                    "task_preview": state.last_cmd.chars().take(240).collect::<String>(),
                    "intent_preview": intent.chars().take(240).collect::<String>(),
                    "turn_mode": state.turn_mode.as_str(),
                    "source_frame_id": state.source_frame_id,
                }),
            );

            if super::reader_policy::guard_tool_call(state, &id, &name, &args, action) {
                return;
            }

            overlay::set_status(format!("doing: {name}"));
            overlay::set_orb_tool(&name, &args);
            let cancel = Arc::new(AtomicBool::new(false));
            state.pending = Pending {
                id: Some(id.clone()),
                cancelled: false,
                cancel: Some(cancel.clone()),
            };
            // Runs on the executor thread (the Brain dispatch + grounding).
            let job = Job {
                id: id.clone(),
                name: name.clone(),
                args,
                task: state.last_cmd.clone(),
                user_text: state.last_user_text.clone(),
                action,
                source_frame_id: state.source_frame_id,
                queued_at: Instant::now(),
                cancel,
            };
            if exec_tx.send(job).is_err() {
                state.pending.request_cancel();
                state.pending = Pending::default();
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
                "server goAway ({time_left}) - reconnecting proactively"
            ));
            state.go_away = true;
        }
        ServerEvent::Usage(usage) => {
            telemetry::event(
                "model_usage",
                "runtime",
                Privacy::Safe,
                serde_json::json!({"usage": usage}),
            );
        }
        _ => {}
    }
}

fn compact_args(args: &Value) -> String {
    let s = args.to_string();
    let clipped: String = s.chars().take(80).collect();
    if clipped.len() < s.len() {
        format!("{clipped}...")
    } else {
        clipped
    }
}
