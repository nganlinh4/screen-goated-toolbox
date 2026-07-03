//! Reader-side session state (the `Reader` state machine + rolling conversation
//! history) and the server-event handler — split out of `runtime.rs` to keep it
//! within the file-size limit.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use serde_json::Value;

use super::super::overlay;
use super::super::playback::AudioSink;
use super::super::protocol::ServerEvent;
use super::Job;
use super::speech_gate::{SpeechGate, TranscriptDecision};

/// The single in-flight tool call (synchronous FC ⇒ at most one), plus whether the
/// server cancelled it (in which case we must NOT answer it).
#[derive(Default)]
pub(super) struct Pending {
    pub(super) id: Option<String>,
    pub(super) cancelled: bool,
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
        eprintln!("[cc] said: {clipped}"); // surface the spoken reply for debugging
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
    cancel: &Arc<AtomicBool>,
    exec_tx: &mpsc::Sender<Job>,
    state: &mut Reader,
) {
    match ev {
        ServerEvent::Audio(pcm) => {
            state.speech_gate.push_audio(&pcm, sink);
        }
        ServerEvent::Interrupted => {
            // Barge-in: stop TALKING so the agent listens, but let the in-flight
            // ACTION finish (the user just wants to comment/steer, not abort the
            // click). Only an explicit "stop" (below) aborts the action.
            if let Some(sink) = sink {
                sink.clear();
            }
        }
        ServerEvent::ToolCancellation(ids) => {
            // The user spoke while a tool call was pending, so the server cancelled
            // it. We must NOT answer that id (invalid), AND we ABORT the in-flight
            // action: the model is about to re-plan from the new input, so letting a
            // now-irrelevant long action (a wait, a vision call, a humanized glide)
            // run to completion just wastes time. This IS our "stop" - it's driven by
            // the server's voice-activity barge-in, which works in ANY language, so
            // no spoken-keyword list is needed.
            if let Some(sink) = sink {
                sink.clear();
            }
            if let Some(p) = state.pending.id.as_ref()
                && ids.iter().any(|i| i == p)
            {
                state.pending.cancelled = true; // don't answer it...
                cancel.store(true, Ordering::SeqCst); // ...and abort the action now
                overlay::set_status("halting...");
            }
            overlay::push_log(format!("[~] halting current step + re-planning {ids:?}"));
        }
        ServerEvent::InputTranscript(t) => {
            // This is Gemini's text of your AUDIO and can mis-hear ("play it" -> "Vai é").
            // The MODEL reasons on the audio, not this text, so we keep it OUT of history and
            // the task/vision context - it drives ONLY the orb caption. The turn's real task is
            // captured from the model's own intent at its first action (see ToolCall below).
            if !t.trim().is_empty() {
                flush_reply(state); // close the assistant's prior reply into history
                state.reasoning.clear(); // this turn's intent starts fresh - don't inherit last turn's narration
                state.thinking.clear();
                state.speech_gate.reset();
                state.last_cmd.clear(); // new turn - the goal is re-derived from the model's intent
                state.has_command = true; // the user has spoken (gates the browser offer)
                state.active = true; // a fresh request - resume pushing frames
                state.awaiting = true; // model now owes a response
                state.think_start = Some(Instant::now()); // start the think-time clock
            }
            overlay::set_user_text(t);
            overlay::set_listening(false);
        }
        ServerEvent::OutputTranscript(t) => {
            // The CLEAN spoken transcript (outputAudioTranscription) — the real
            // "voice". This is what SGT's canonical Live path records.
            match state.speech_gate.transcript(&t, sink) {
                TranscriptDecision::Allow(text) => {
                    state.reasoning.push_str(text); // per-action intent (cleared each tool call)
                    state.reply.push_str(text); // spoken reply -> history + `said:` log
                    // Caption shows the WHOLE reply so far (outputTranscription arrives as deltas) - so it
                    // grows word-by-word instead of cutting to just the latest chunk.
                    overlay::set_model_text(state.reply.clone());
                }
                TranscriptDecision::Block => {
                    state.reasoning.clear();
                    state.reply.clear();
                    overlay::set_model_text(String::new());
                    overlay::push_log(
                        "[speech-filter] suppressed internal/tool-plan speech".to_string(),
                    );
                }
            }
        }
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
            // The model finished a turn. If it was mid-task and produced NO tool call,
            // it narrated INSTEAD of acting (the "it stopped" failure) - flag it + the
            // gap so we can see it in the log.
            if state.active
                && let Some(t) = state.think_start.take()
            {
                state.stall_count += 1;
                overlay::push_log(format!(
                    "[~] turn ended after {}ms mid-task with NO action (narrated only)",
                    t.elapsed().as_millis()
                ));
            }
            state.speech_gate.finish_turn(sink);
            flush_reply(state);
            state.awaiting = false;
        }
        ServerEvent::ToolCall { id, name, args } => {
            state.speech_gate.finish_turn(sink);
            state.awaiting = false; // model responded (with an action)
            if let Some(t) = state.think_start.take() {
                let ms = t.elapsed().as_millis();
                let spoke = !state.reasoning.trim().is_empty(); // narrated this turn? (~2s tax)
                if spoke {
                    let preview: String = state.reasoning.trim().chars().take(220).collect();
                    overlay::push_log(format!(
                        "[speech before {name}] {} chars: {preview}",
                        state.reasoning.trim().chars().count()
                    ));
                }
                overlay::push_log(format!(
                    "[think {ms}ms{}]",
                    if spoke { ", spoke" } else { "" }
                ));
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
            overlay::push_log(format!(">{name} {}", compact_args(&args)));
            overlay::set_status(format!("doing: {name}"));
            overlay::set_orb_tool(&name, &args);
            state.pending = Pending {
                id: Some(id.clone()),
                cancelled: false,
            };
            // Runs on the executor thread (the Brain dispatch + grounding).
            let _ = exec_tx.send((id, name, args, state.last_cmd.clone(), intent));
        }
        ServerEvent::GoAway { time_left } => {
            overlay::push_log(format!(
                "server goAway ({time_left}) - reconnecting proactively"
            ));
            state.go_away = true;
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
