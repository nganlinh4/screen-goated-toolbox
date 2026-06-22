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

mod reader;
use reader::{Pending, Reader, build_recap, flush_reply, handle_event, record_observation};

/// How often a fresh (gridded) screenshot is pushed while idle.
const FRAME_INTERVAL: Duration = Duration::from_millis(1800);
const MAX_RECONNECTS: u32 = 6;
/// The preview Live model often goes silent mid-turn without closing the socket.
/// When it owes us a response and we've heard nothing for `NUDGE_SILENCE`, poke it
/// with a fresh frame (cheap, keeps session memory). Only if it's STILL silent at
/// `RECONNECT_SILENCE` do we tear down + reconnect (which drops in-flight context).
const NUDGE_SILENCE: Duration = Duration::from_secs(7);
const RECONNECT_SILENCE: Duration = Duration::from_secs(18);

/// A tool call handed to the executor thread: (id, name, args, task, intent).
type Job = (String, String, Value, String, String);
/// A finished action from the executor: (id, name, response, optional frame b64).
type Done = (String, String, Value, Option<String>);

pub(super) fn run(stop: Arc<AtomicBool>) {
    match run_inner(&stop) {
        Ok(()) => overlay::set_status("stopped"),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if stop.load(Ordering::SeqCst) || msg == "stopped" {
                // You stopped during connect/setup (e.g. toggling the hotkey fast) -
                // a clean shutdown, NOT an error.
                overlay::set_status("stopped");
            } else if msg.contains("quota") || msg.contains("exceeded") || msg.contains("resource_exhausted") {
                overlay::push_log(
                    "Gemini rate limit hit (a burst of Live connections). This is usually the per-minute / \
concurrent-session cap, NOT your daily quota - just WAIT ~30-60s and start again. If it persists, check the key \
matches your AI Studio project, or use a billing-enabled key."
                        .to_string(),
                );
                overlay::set_status("rate limited - wait ~1 min and retry");
            } else {
                overlay::push_log(format!("[warn] session error: {e}"));
                overlay::set_status("error");
            }
        }
    }
    overlay::set_listening(false);
}

fn run_inner(stop: &Arc<AtomicBool>) -> anyhow::Result<()> {
    let key = session::load_key()?;
    let target = std::env::var("CC_UIA_WINDOW").ok();
    overlay::set_status("connecting...");

    // AUDIO FIRST: cpal/WASAPI must claim this thread's COM apartment BEFORE the
    // WebSocket's TLS (or UIA) initializes COM in a conflicting mode - otherwise
    // building the mic stream fails with RPC_E_CHANGED_MODE ("cannot change thread
    // mode after it is set").
    let mic_buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let mic_pause = Arc::new(AtomicBool::new(false));
    // Build the mic stream, retrying a few times (WASAPI can transiently report
    // "insufficient system resources" while a device is being reassigned).
    let init_mic = || -> anyhow::Result<cpal::Stream> {
        let mut attempt = 0;
        loop {
            match crate::api::realtime_audio::start_mic_capture(
                mic_buf.clone(),
                stop.clone(),
                mic_pause.clone(),
            ) {
                Ok(s) => return Ok(s),
                Err(_) if attempt < 4 => {
                    attempt += 1;
                    overlay::push_log(format!("(audio device busy - retrying {attempt}/4)"));
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => return Err(e),
            }
        }
    };
    let mut _mic_stream = init_mic()?;
    let mut sink = AudioSink::new(); // output voice (24 kHz); optional
    if sink.is_none() {
        overlay::push_log("(no audio output device - replies shown as text only)".to_string());
    }
    // Track the default input device so we can RE-INIT mic + output if it changes
    // mid-session (e.g. you plug in headphones), instead of going silently deaf.
    let mut audio_device = crate::api::realtime_audio::current_input_device_name();
    let mut last_device_check = Instant::now();

    // Try WITH Google Search grounding; if setup is rejected (grounding needs a
    // billing-enabled project / quota), fall back to a search-less session so it
    // still starts. Other Live features don't use search, which is why they work.
    let mut socket = connect_ws(&key)?;
    send(&mut socket, uia_task::build_setup(None, true, true))?;
    if wait_for_setup(&mut socket, stop).is_err() {
        let _ = socket.close(None);
        overlay::push_log("(Google Search unavailable on this key — starting without it)".to_string());
        socket = connect_ws(&key)?;
        send(&mut socket, uia_task::build_setup(None, true, false))?;
        wait_for_setup(&mut socket, stop)?;
    }
    set_socket_nonblocking(&mut socket)?;
    overlay::set_status("ready - speak a command");
    overlay::push_log("* connected; sending your WHOLE screen + mic each turn (smart brain)".to_string());

    // Steer/stop core: the Brain + its (possibly slow) actions run on a SEPARATE
    // thread so the reader keeps receiving mic + barge-in WHILE an action runs.
    // CANCEL is flipped on barge-in; the humanized executor polls it between
    // micro-steps so a spoken "stop" halts mid-glide. Synchronous FC ⇒ the model
    // is blocked awaiting our toolResponse, so we ALWAYS answer the pending id
    // (unless the server itself cancelled it) or the session deadlocks.
    // Bring up the browser-control bridge server so the extension (if installed)
    // can connect; idempotent across sessions.
    super::browser::ensure_started();

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
    let mut last_event = Instant::now();
    let mut state = Reader::default();
    let mut reconnects = 0u32;
    // Representative clean frames captured across the session (deduped by window
    // title, newest 6 kept) - embedded with the transcript into conversation
    // memory so a past session is findable by what it LOOKED like, not just words.
    let mut mem_frames: Vec<Vec<u8>> = Vec::new();
    let mut last_mem_title = String::new();
    let mut last_mem_check = Instant::now();
    // One-time proactive offer to set up deep browser control when the user is
    // browsing without it (and hasn't recently declined).
    let mut offered_browser = false;
    let mut last_offer_check = Instant::now();
    // By default the mic stays OPEN while the agent talks, so you can barge in and
    // interrupt its speech (native Live behaviour). On open speakers (no headphones
    // / no echo cancellation) the agent's own voice can leak into the mic and make
    // it interrupt itself - set CC_MIC_GATE=1 to mute the mic during playback.
    let echo_gate = std::env::var("CC_MIC_GATE").is_ok();

    while !stop.load(Ordering::SeqCst) {
        // 0) default audio device changed (e.g. headphones in/out)? re-init mic +
        //    output on the NEW device so the session keeps hearing/speaking.
        if last_device_check.elapsed() >= Duration::from_secs(2) {
            last_device_check = Instant::now();
            let now_device = crate::api::realtime_audio::current_input_device_name();
            if now_device != audio_device {
                overlay::push_log(format!(
                    "(audio device changed -> {} - re-initializing)",
                    now_device.as_deref().unwrap_or("none")
                ));
                audio_device = now_device;
                std::thread::sleep(Duration::from_millis(300)); // let the new device settle
                match init_mic() {
                    Ok(s) => {
                        _mic_stream = s; // dropping the old stream releases the old device
                        sink = AudioSink::new();
                    }
                    Err(e) => overlay::push_log(format!("(mic re-init failed: {e})")),
                }
            }
        }

        // 0b) capture a representative clean frame each time the foreground window
        //     changes, for conversation memory (keep the newest 6 distinct screens).
        if last_mem_check.elapsed() >= Duration::from_secs(3) {
            last_mem_check = Instant::now();
            let title = super::uia::pointer_context().0;
            if !title.is_empty() && title != last_mem_title {
                last_mem_title = title;
                if let Ok((jpeg, _)) = session::capture_frame_jpeg() {
                    mem_frames.push(jpeg);
                    if mem_frames.len() > 6 {
                        mem_frames.remove(0);
                    }
                }
            }
        }

        // 0c) one-time proactive offer: the user is browsing, deep control isn't
        //     set up, and they haven't recently declined → nudge the model to offer
        //     (it phrases it in the user's language). Only when idle + mid-session.
        if !offered_browser
            && !state.active
            && !state.awaiting
            && !state.last_cmd.trim().is_empty()
            && last_event.elapsed() > Duration::from_secs(6) // genuinely idle, not mid-request
            && last_offer_check.elapsed() >= Duration::from_secs(4)
        {
            last_offer_check = Instant::now();
            if !super::browser::is_connected()
                && super::browser::offer_due()
                && foreground_is_browser()
            {
                offered_browser = true;
                let _ = send(
                    &mut socket,
                    realtime_text(
                        "(Heads-up for you, not the user: they're working in a web browser and deep browser control \
isn't set up. If it fits the moment, briefly offer ONCE - in their language - to set it up via browser_setup for \
more precise page reading/acting. If they decline, call decline_browser_control.)",
                    ),
                );
                state.awaiting = true; // expect the model to speak the offer
            }
        }

        // 0d) the server warned the session is ending (goAway). Reconnect PROACTIVELY
        //     at the next gap (no tool call in flight) so we migrate the conversation
        //     cleanly with our recap - instead of being force-closed mid-stream (which
        //     dropped us with a gap + a "client failed to close" error).
        if state.go_away && state.pending.id.is_none() {
            state.go_away = false;
            overlay::push_log("(goAway) reconnecting before the session ends".to_string());
            if !reconnect_session(&mut socket, &key, target.as_deref(), &mut reconnects, &mut state)? {
                break;
            }
            last_event = Instant::now();
            last_frame = Instant::now();
            continue;
        }

        // 1) mic -> server. Open during TTS so you can barge in, unless echo_gate.
        let chunk = {
            let mut b = mic_buf.lock().unwrap();
            std::mem::take(&mut *b)
        };
        let muted = echo_gate && sink.as_ref().map(|s| s.is_playing()).unwrap_or(false);
        if !chunk.is_empty() && !muted {
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
                record_observation(&mut state, &name, &resp); // durable memory of what we saw
                send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                if let Some(f) = frame {
                    let _ = send(&mut socket, realtime_video_jpeg_b64(&f)); // then frame
                }
                // An accepted `done` ends the request: go idle (stop pushing frames)
                // until the user speaks again. A rejected done keeps working.
                if name == "done" && resp_ok {
                    overlay::push_log("[done] goal reached".to_string());
                    state.active = false;
                    state.awaiting = false;
                } else {
                    state.awaiting = true; // model owes the next action/turn
                    state.think_start = Some(Instant::now()); // measure the next think-time
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
                // Staleness recovery: the preview Live model often goes SILENT without
                // closing the socket. Only relevant while it OWES us a turn and nothing
                // is in flight (`pending.id.is_none()` ⇒ never fires during a slow
                // vision call). Recover gently first: a NUDGE (fresh frame + a terse
                // "continue") usually un-sticks it WITHOUT losing session memory; only
                // if it stays silent do we fall back to the context-dropping reconnect.
                if state.awaiting && state.pending.id.is_none() {
                    let silent = last_event.elapsed();
                    if silent > RECONNECT_SILENCE {
                        overlay::push_log("(session still silent - reconnecting)".to_string());
                        if !reconnect_session(&mut socket, &key, target.as_deref(), &mut reconnects, &mut state)? {
                            break;
                        }
                        last_event = Instant::now();
                        last_frame = Instant::now();
                        continue;
                    } else if silent > NUDGE_SILENCE && !state.nudged {
                        // One poke per silent spell, then escalate. Send ONLY a fresh
                        // frame - never an injected "continue" instruction. A long
                        // answer makes the model go silent while it THINKS, not because
                        // it's stuck; a text nudge there gets queued as a second user
                        // turn, so the model answers, then re-answers (restarting the
                        // story). A bare frame is the same ambient input we already
                        // stream, so it can't be mistaken for a new request.
                        state.nudged = true;
                        overlay::push_log("(nudging the model with a fresh frame)".to_string());
                        if let Ok(f) = uia_task::snapshot(target.as_deref()) {
                            let _ = send(&mut socket, realtime_video_jpeg_b64(&f));
                        }
                    }
                }
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
        last_event = Instant::now(); // heard from the server - session is alive
        state.nudged = false; // silence broken - re-arm the nudge for next time
        for ev in parse_server_message(&text) {
            handle_event(ev, sink.as_ref(), &cancel, &exec_tx, &mut state);
        }
    }
    // Persist the whole session to searchable memory (saved + embedded on a
    // detached thread, so this returns immediately). The agent can recall it in a
    // future session via search_memory/open_memory.
    flush_reply(&mut state); // close the final spoken reply into the transcript
    super::memory::save(state.history.clone(), std::mem::take(&mut mem_frames));

    // On stop, abort any in-flight action so the executor frees up promptly - else
    // join() blocks (up to a slow vision call) and the mic/audio client lingers,
    // accumulating across session restarts until WASAPI runs out of resources.
    cancel.store(true, Ordering::SeqCst);
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
    match uia_task::reconnect(key, None, true, false) {
        Ok(s) => *socket = s,
        Err(e) => {
            overlay::push_log(format!("reconnect failed: {e}"));
            return Ok(false);
        }
    }
    state.pending = Pending::default();
    state.nudged = false; // fresh session - re-arm the nudge
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

/// Best-effort: is the foreground window a web browser? (Brand names in window
/// titles are language-stable, e.g. "… - Google Chrome", "… - Microsoft Edge".)
fn foreground_is_browser() -> bool {
    let title = super::uia::pointer_context().0.to_lowercase();
    ["chrome", "edge", "brave", "opera", "firefox", "chromium", "vivaldi"]
        .iter()
        .any(|b| title.contains(b))
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
