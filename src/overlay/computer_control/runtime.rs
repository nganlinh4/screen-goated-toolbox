//! Continuous Computer Control voice session: connect, stream mic + screen, and
//! drive the shared `Brain` (UIA grounding + Set-of-Mark grid + vision locate +
//! robustness - the SAME brain the headless harness uses) from a dedicated
//! executor thread, so a slow humanized action can run while the reader thread
//! keeps receiving mic + barge-in. A spoken "stop" flips CANCEL and halts
//! SendInput mid-glide.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::Message;

use crate::api::realtime_audio::websocket::{
    is_transient_socket_read_error, send_audio_chunk, set_socket_nonblocking,
    set_socket_short_timeout,
};

use super::overlay;
use super::playback::AudioSink;
use super::protocol::{
    ServerEvent, parse_server_message, realtime_text, realtime_video_jpeg_b64, tool_response,
};
use super::session::{self, Sock, connect_ws, send};
use super::uia_task::{self, Brain};

/// How often a fresh (gridded) screenshot is pushed while idle.
const FRAME_INTERVAL: Duration = Duration::from_millis(1800);
const MAX_RECONNECTS: u32 = 6;

/// A tool call handed to the executor thread: (id, name, args, task, intent).
type Job = (String, String, Value, String, String);
/// A finished action from the executor: (id, name, response, optional frame b64).
type Done = (String, String, Value, Option<String>);

pub(super) fn run(stop: Arc<AtomicBool>) {
    match run_inner(&stop) {
        Ok(()) => overlay::set_status("stopped"),
        Err(e) => {
            overlay::push_log(format!("[warn] session error: {e}"));
            overlay::set_status("error");
        }
    }
    overlay::set_listening(false);
}

fn run_inner(stop: &Arc<AtomicBool>) -> anyhow::Result<()> {
    let key = session::load_key()?;
    let target = std::env::var("CC_UIA_WINDOW").ok();
    overlay::set_status("connecting...");
    let mut socket = connect_ws(&key)?;
    send(&mut socket, uia_task::build_setup(None, true))?;
    wait_for_setup(&mut socket, stop)?;
    set_socket_nonblocking(&mut socket)?;
    overlay::set_status("ready - speak a command");
    overlay::push_log("* connected; streaming screen + mic (smart brain)".to_string());

    // Mic (16 kHz mono i16) into a shared buffer.
    let mic_buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let mic_pause = Arc::new(AtomicBool::new(false));
    let _mic_stream =
        crate::api::realtime_audio::start_mic_capture(mic_buf.clone(), stop.clone(), mic_pause)?;

    // Output voice (24 kHz). Optional - run muted if there is no output device.
    let sink = AudioSink::new();
    if sink.is_none() {
        overlay::push_log("(no audio output device - replies shown as text only)".to_string());
    }

    // Steer/stop core: the Brain + its (possibly slow) actions run on a SEPARATE
    // thread so the reader keeps receiving mic + barge-in WHILE an action runs.
    // CANCEL is flipped on barge-in; the humanized executor polls it between
    // micro-steps so a spoken "stop" halts mid-glide. Synchronous FC ⇒ the model
    // is blocked awaiting our toolResponse, so we ALWAYS answer the pending id
    // (unless the server itself cancelled it) or the session deadlocks.
    let cancel = Arc::new(AtomicBool::new(false));
    let (exec_tx, exec_rx) = mpsc::channel::<Job>();
    let (res_tx, res_rx) = mpsc::channel::<Done>();
    let exec_cancel = cancel.clone();
    let exec_target = target.clone();
    let exec_thread = std::thread::spawn(move || executor_loop(exec_target, exec_rx, res_tx, exec_cancel));

    let f0 = uia_task::snapshot(target.as_deref()).unwrap_or_default();
    if !f0.is_empty() {
        send(&mut socket, realtime_video_jpeg_b64(&f0))?;
    }
    let mut last_frame = Instant::now();
    let mut state = Reader::default();
    let mut reconnects = 0u32;

    while !stop.load(Ordering::SeqCst) {
        // 1) mic -> server, GATED while our own TTS plays (avoid self-barge-in).
        let chunk = {
            let mut b = mic_buf.lock().unwrap();
            std::mem::take(&mut *b)
        };
        let speaking = sink.as_ref().map(|s| s.is_playing()).unwrap_or(false);
        if !chunk.is_empty() && !speaking {
            overlay::set_listening(true);
            send_audio_chunk(&mut socket, &chunk)?;
        }

        // 2) periodic gridded frame WHILE a request is active (so the agent keeps
        //    seeing the screen as it works, but goes quiet - and stops acting -
        //    once a request is done, until the user speaks again).
        if state.active && state.pending.id.is_none() && last_frame.elapsed() >= FRAME_INTERVAL {
            if let Ok(f) = uia_task::snapshot(target.as_deref()) {
                let _ = send(&mut socket, realtime_video_jpeg_b64(&f));
            }
            last_frame = Instant::now();
        }

        // 3) executor finished an action -> answer the tool (+ push the new frame).
        if let Ok((id, name, resp, frame)) = res_rx.try_recv()
            && state.pending.id.as_deref() == Some(id.as_str())
        {
            if state.pending.cancelled {
                // The action finished (or was stopped); its result is dropped
                // because you spoke and the model already moved on.
                overlay::push_log("[~] step done; result dropped (you spoke)".to_string());
            } else {
                let resp_ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                if let Some(f) = frame {
                    let _ = send(&mut socket, realtime_video_jpeg_b64(&f)); // then frame
                }
                // An accepted `done` ends the request: go idle (stop pushing frames)
                // until the user speaks again. A rejected done keeps working.
                if name == "done" && resp_ok {
                    overlay::push_log("[done] goal reached".to_string());
                    state.active = false;
                }
            }
            state.pending = Pending::default();
            cancel.store(false, Ordering::SeqCst);
            last_frame = Instant::now();
            overlay::set_status("ready - speak a command");
        }

        // 4) read one event (reconnect on unexpected close/error).
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(frame)) => {
                overlay::push_log(format!("socket closed: {frame:?} - reconnecting"));
                if !reconnect_session(&mut socket, &key, target.as_deref(), &mut reconnects, &mut state)? {
                    break;
                }
                last_frame = Instant::now();
                continue;
            }
            Ok(_) => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) if is_transient_socket_read_error(&e) => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => {
                overlay::push_log(format!("read error: {e} - reconnecting"));
                if !reconnect_session(&mut socket, &key, target.as_deref(), &mut reconnects, &mut state)? {
                    break;
                }
                last_frame = Instant::now();
                continue;
            }
        };
        reconnects = 0; // healthy read - reset the budget
        for ev in parse_server_message(&text) {
            handle_event(ev, sink.as_ref(), &cancel, &exec_tx, &mut state);
        }
    }
    drop(exec_tx); // close the channel -> executor thread exits
    let _ = exec_thread.join();
    Ok(())
}

/// The executor thread: owns the `Brain` and runs every tool call (including the
/// independent `done` verification) off the reader thread.
fn executor_loop(target: Option<String>, rx: mpsc::Receiver<Job>, tx: mpsc::Sender<Done>, cancel: Arc<AtomicBool>) {
    let mut brain = Brain::new(target);
    while let Ok((id, name, args, task, intent)) = rx.recv() {
        cancel.store(false, Ordering::SeqCst); // each action starts fresh
        let done: Done = if name == "done" {
            // Independent high-res check - the Live agent confabulates success.
            let (ok, verdict) = brain.verify_done(&task, &cancel);
            if ok {
                (id, name, json!({"ok": true, "verdict": verdict}), None)
            } else {
                let (state_text, frame) = match brain.ground(&name, &args) {
                    Ok(g) => (g.state_text, Some(g.frame_b64)),
                    Err(e) => (format!("(ground failed: {e})"), None),
                };
                (
                    id,
                    name,
                    json!({
                        "ok": false,
                        "independent_check": verdict,
                        "instruction": "An independent high-res check says the goal is NOT yet achieved. Keep \
working until it is actually done.",
                        "new_state": state_text,
                    }),
                    frame,
                )
            }
        } else {
            let ctx = format!(
                "task: {task}; agent intent: {}",
                if intent.is_empty() { "(none stated)" } else { intent.as_str() }
            );
            let action_result = brain.dispatch(&name, &args, &ctx, &cancel);
            match brain.ground(&name, &args) {
                Ok(g) => {
                    let mut resp = json!({"action_result": action_result, "new_state": g.state_text});
                    for (k, v) in &g.notes {
                        resp[*k] = json!(*v);
                    }
                    (id, name, resp, Some(g.frame_b64))
                }
                Err(e) => (id, name, json!({"action_result": action_result, "ground_error": e.to_string()}), None),
            }
        };
        if tx.send(done).is_err() {
            break;
        }
    }
}

/// Reconnect to a FRESH session (resumption is rejected on this preview model) and
/// re-seed the current screen PLUS a recap of the conversation so far, so the agent
/// keeps its memory across the drop. Returns false to give up. Clears the pending
/// tool (the new session has no memory of it).
fn reconnect_session(
    socket: &mut Sock,
    key: &str,
    target: Option<&str>,
    reconnects: &mut u32,
    state: &mut Reader,
) -> anyhow::Result<bool> {
    *reconnects += 1;
    if *reconnects > MAX_RECONNECTS {
        overlay::push_log(format!("giving up after {MAX_RECONNECTS} reconnects"));
        return Ok(false);
    }
    overlay::set_status("reconnecting...");
    match uia_task::reconnect(key, None, true) {
        Ok(s) => *socket = s,
        Err(e) => {
            overlay::push_log(format!("reconnect failed: {e}"));
            return Ok(false);
        }
    }
    state.pending = Pending::default();
    flush_reply(state); // capture any in-flight reply before recapping
    if let Ok(f) = uia_task::snapshot(target) {
        send(socket, realtime_video_jpeg_b64(&f))?;
    }
    let recap = build_recap(&state.history);
    let msg = if recap.is_empty() {
        "(reconnected after a dropped connection) Continue helping with the user's latest request, based on the \
current screen."
            .to_string()
    } else {
        format!(
            "(reconnected after a dropped connection) Here is our conversation so far - keep this context and \
continue:\n{recap}\n\nContinue from the CURRENT screen."
        )
    };
    send(socket, realtime_text(&msg))?;
    overlay::push_log("(reconnected - conversation memory restored)".to_string());
    overlay::set_status("ready - speak a command");
    Ok(true)
}

/// The single in-flight tool call (synchronous FC ⇒ at most one), plus whether the
/// server cancelled it (in which case we must NOT answer it).
#[derive(Default)]
struct Pending {
    id: Option<String>,
    cancelled: bool,
}

/// Mutable reader-side session state threaded through `handle_event`.
#[derive(Default)]
struct Reader {
    pending: Pending,
    /// The model's spoken output since the last tool call - its "intent" context.
    reasoning: String,
    /// The latest spoken user command - the task context handed to vision.
    last_cmd: String,
    /// True while a spoken request is being worked on. Idle frames are pushed only
    /// while active, so after `done` the agent waits for the user instead of
    /// treating each new frame as a cue to keep acting.
    active: bool,
    /// Rolling conversation history (alternating "User:"/"Assistant:" lines). The
    /// preview model rejects sessionResumption, so on a dropped connection we
    /// re-seed a fresh session with this recap - the agent keeps its memory.
    history: Vec<String>,
    /// The assistant's spoken reply since the last user turn, flushed into
    /// `history` when the user speaks again (or on reconnect).
    reply: String,
}

/// Cap on history entries kept (rolling); older turns drop off.
const MAX_HISTORY: usize = 24;
/// Cap on the recap text seeded on reconnect (kept well under the 1007
/// "invalid argument" size threshold).
const RECAP_BUDGET: usize = 1500;

/// Close out the assistant's accumulated reply into the conversation history.
fn flush_reply(state: &mut Reader) {
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

/// Build a recap of the most recent conversation (newest-biased, length-capped).
fn build_recap(history: &[String]) -> String {
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

fn handle_event(
    ev: ServerEvent,
    sink: Option<&AudioSink>,
    cancel: &Arc<AtomicBool>,
    exec_tx: &mpsc::Sender<Job>,
    state: &mut Reader,
) {
    match ev {
        ServerEvent::Audio(pcm) => {
            if let Some(sink) = sink {
                sink.push(&pcm);
            }
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
            // The server discarded the pending call because new user input arrived.
            // We must NOT answer that id (would be invalid) - but we let the action
            // run to completion so the move still happens; only the result is
            // dropped. The model re-plans from the user's new input.
            if let Some(sink) = sink {
                sink.clear();
            }
            if let Some(p) = state.pending.id.as_ref()
                && ids.iter().any(|i| i == p)
            {
                state.pending.cancelled = true; // don't answer; action still finishes
            }
            overlay::push_log(format!("[~] re-planning (current step still finishing) {ids:?}"));
        }
        ServerEvent::InputTranscript(t) => {
            // Local fast-path: a spoken stop halts NOW, before the round-trip.
            let lt = t.to_lowercase();
            if state.pending.id.is_some()
                && (lt.contains("stop") || lt.contains("dừng") || lt.contains("wait"))
            {
                cancel.store(true, Ordering::SeqCst); // explicit stop aborts the action
                overlay::set_status("halting...");
                overlay::push_log("[stop] halting on your command".to_string());
            }
            if !t.trim().is_empty() {
                flush_reply(state); // close the assistant's prior reply into history
                state.history.push(format!("User: {}", t.trim()));
                if state.history.len() > MAX_HISTORY {
                    let drop = state.history.len() - MAX_HISTORY;
                    state.history.drain(0..drop);
                }
                state.last_cmd = t.clone(); // task context for vision
                state.active = true; // a fresh request - resume pushing frames
            }
            overlay::set_user_text(t);
            overlay::set_listening(false);
        }
        ServerEvent::OutputTranscript(t) => {
            // The CLEAN spoken transcript (outputAudioTranscription) — the real
            // "voice". This is what SGT's canonical Live path records.
            state.reasoning.push_str(&t); // per-action intent (cleared each tool call)
            state.reply.push_str(&t); // spoken reply -> history + `said:` log
            overlay::set_model_text(t);
        }
        ServerEvent::ModelText(_) => {
            // modelTurn text parts in AUDIO mode carry tool-call / internal text
            // (e.g. "call:look{...}"), NOT spoken words — ignore so they don't
            // pollute the spoken transcript or the vision intent context.
        }
        ServerEvent::TurnComplete => {
            // The model finished a turn — record its spoken reply now (clean,
            // real-time) rather than waiting for the next user utterance.
            flush_reply(state);
        }
        ServerEvent::ToolCall { id, name, args } => {
            let intent = state.reasoning.trim().to_string();
            state.reasoning.clear();
            overlay::push_log(format!(">{name} {}", compact_args(&args)));
            overlay::set_status(format!("doing: {name}"));
            state.pending = Pending { id: Some(id.clone()), cancelled: false };
            // Runs on the executor thread (the Brain dispatch + grounding).
            let _ = exec_tx.send((id, name, args, state.last_cmd.clone(), intent));
        }
        ServerEvent::GoAway { time_left } => {
            overlay::push_log(format!("server goAway ({time_left}) - session will end"))
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

fn wait_for_setup(socket: &mut Sock, stop: &Arc<AtomicBool>) -> anyhow::Result<()> {
    set_socket_short_timeout(socket)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !stop.load(Ordering::SeqCst) {
        if Instant::now() > deadline {
            anyhow::bail!("timed out waiting for setupComplete");
        }
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(frame)) => anyhow::bail!("server closed during setup: {frame:?}"),
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => anyhow::bail!("setup read error: {e}"),
        };
        for ev in parse_server_message(&text) {
            if matches!(ev, ServerEvent::SetupComplete) {
                return Ok(());
            }
        }
    }
    anyhow::bail!("stopped")
}
