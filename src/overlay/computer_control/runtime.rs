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

use serde_json::Value;
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
use super::telemetry::{self, Privacy};
use super::uia_task;

mod action_worker;
mod mic;
mod reader;
mod speech_gate;
use action_worker::executor_loop;
use mic::mic_thread;
use reader::{Pending, Reader, build_recap, flush_reply, handle_event, record_observation};

/// How often a fresh (gridded) screenshot is streamed while you're talking or a request is
/// active. A frame is ALSO pushed immediately on speech onset so it leads the turn (the model
/// needs the frame ingested before the turn closes, or it sees no image).
const FRAME_INTERVAL: Duration = Duration::from_millis(1800);
const MAX_RECONNECTS: u32 = 6;
/// The preview Live model often goes silent mid-turn without closing the socket.
/// When it owes us a response and we've heard nothing for `NUDGE_SILENCE`, poke it
/// with a fresh frame (cheap, keeps session memory). Only if it's STILL silent at
/// `RECONNECT_SILENCE` do we tear down + reconnect (which drops in-flight context).
/// RECONNECT is deliberately GENEROUS: this model legitimately THINKS for 20-30s on a
/// complex turn, and reconnecting mid-think drops its working context and sends it
/// flailing (clicking the wrong thing, redoing work) - far worse than waiting a bit
/// longer for a genuinely hung session. Don't drop this below the real think latency.
const NUDGE_SILENCE: Duration = Duration::from_secs(8);
const RECONNECT_SILENCE: Duration = Duration::from_secs(40);

/// A tool call handed to the executor thread: (id, name, args, task, intent).
type Job = (String, String, Value, String, String);
/// A finished action from the executor: (id, name, response, optional frame b64).
type Done = (String, String, Value, Option<String>);

/// Sink for typed commands from the orb's text box → the live session, set on each session
/// start. The orb thread calls [`submit_text_command`]; `run_inner` drains it each loop.
static TEXT_COMMAND_TX: Mutex<Option<mpsc::Sender<String>>> = Mutex::new(None);

/// Submit a typed command (from the orb's text box) into the active session as a user text
/// turn — lets you drive the agent silently in a quiet place. No-op when no session is up.
pub(super) fn submit_text_command(text: String) {
    if let Ok(g) = TEXT_COMMAND_TX.lock()
        && let Some(tx) = g.as_ref()
    {
        let _ = tx.send(text);
    }
}

pub(super) fn run(stop: Arc<AtomicBool>) {
    match run_inner(&stop) {
        Ok(()) => overlay::set_status("stopped"),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if stop.load(Ordering::SeqCst) || msg == "stopped" {
                // You stopped during connect/setup (e.g. toggling the hotkey fast) -
                // a clean shutdown, NOT an error.
                overlay::set_status("stopped");
            } else if msg.contains("quota")
                || msg.contains("exceeded")
                || msg.contains("resource_exhausted")
            {
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

    // The MIC runs on its OWN thread (`mic_thread`): it owns cpal's per-thread COM apartment, the
    // stream, and every default-device rebuild. That isolates it from this loop's TLS/UIA COM churn —
    // a reconnect would otherwise change the apartment mode out from under a mic re-init and trip
    // RPC_E_CHANGED_MODE on a device switch, going silently deaf.
    let mic_buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let mic_pause = Arc::new(AtomicBool::new(false));
    {
        let buf = mic_buf.clone();
        let pause = mic_pause.clone();
        let mic_stop = Arc::clone(stop);
        std::thread::spawn(move || mic_thread(buf, pause, mic_stop));
    }
    let sink = AudioSink::new(); // output voice (24 kHz); the TTS player self-heals on output-device change. optional
    if sink.is_none() {
        overlay::push_log("(no audio output device - replies shown as text only)".to_string());
    }

    // Try WITH Google Search grounding; if setup is rejected (grounding needs a
    // billing-enabled project / quota), fall back to a search-less session so it
    // still starts. Other Live features don't use search, which is why they work.
    let mut socket = connect_ws(&key)?;
    send(&mut socket, uia_task::build_setup(None, true, true))?;
    if wait_for_setup(&mut socket, stop).is_err() {
        let _ = socket.close(None);
        overlay::push_log(
            "(Google Search unavailable on this key — starting without it)".to_string(),
        );
        socket = connect_ws(&key)?;
        send(&mut socket, uia_task::build_setup(None, true, false))?;
        wait_for_setup(&mut socket, stop)?;
    }
    set_socket_nonblocking(&mut socket)?;
    overlay::set_status("ready - speak a command");
    overlay::set_orb_resting();
    overlay::push_log(
        "* connected; sending your WHOLE screen + mic each turn (smart brain)".to_string(),
    );

    // Steer/stop core: the Brain + its (possibly slow) actions run on a SEPARATE
    // thread so the reader keeps receiving mic + barge-in WHILE an action runs.
    // CANCEL is flipped on barge-in; the humanized executor polls it between
    // micro-steps so a spoken "stop" halts mid-glide. Synchronous FC ⇒ the model
    // is blocked awaiting our toolResponse, so we ALWAYS answer the pending id
    // (unless the server itself cancelled it) or the session deadlocks.
    // Bring up the browser-control bridge server so the extension (if installed)
    // can connect; idempotent across sessions.
    super::browser::ensure_started();
    // Bring any installed MCP app-control integrations back online (each on its own
    // thread, since a cold spawn can block); the reconnect-on-tools-changed gap then
    // re-runs build_setup to declare their tools.
    super::mcp::connect_all_installed();

    let cancel = Arc::new(AtomicBool::new(false));
    let (exec_tx, exec_rx) = mpsc::channel::<Job>();
    let (res_tx, res_rx) = mpsc::channel::<Done>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    *TEXT_COMMAND_TX.lock().unwrap() = Some(cmd_tx);
    let exec_cancel = cancel.clone();
    let exec_target = target.clone();
    let exec_thread =
        std::thread::spawn(move || executor_loop(exec_target, exec_rx, res_tx, exec_cancel));

    let f0 = uia_task::snapshot(target.as_deref()).unwrap_or_default();
    if !f0.is_empty() {
        send(&mut socket, realtime_video_jpeg_b64(&f0))?;
    }

    // Screen capture costs ~1-2s/frame here (PrintWindow forces a full re-render of GPU/browser
    // windows), so run it on a BACKGROUND thread into a latest-wins slot. The session loop only ever
    // reads the cached frame (instant) — it must NEVER block on capture, or it stalls the reply-audio
    // pump (which is what made speech choppy + responses slow). `capture_on` idles it when nothing's
    // happening so it doesn't peg a core.
    let frame_slot: Arc<Mutex<Option<String>>> =
        Arc::new(Mutex::new(Some(f0).filter(|s| !s.is_empty())));
    let capture_on = Arc::new(AtomicBool::new(true));
    {
        let slot = frame_slot.clone();
        let on = capture_on.clone();
        let cap_stop = Arc::clone(stop);
        let cap_target = target.clone();
        std::thread::spawn(move || {
            while !cap_stop.load(Ordering::SeqCst) {
                if on.load(Ordering::SeqCst) {
                    if let Ok(f) = uia_task::snapshot(cap_target.as_deref()) {
                        *slot.lock().unwrap() = Some(f);
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(120));
                }
            }
        });
    }

    let mut last_frame = Instant::now();
    let mut last_voice = Instant::now();
    let mut was_playing = false;
    let mut speech_quiet: Option<Instant> = None;
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
    // Proactive offer for a curated MCP app integration when the foreground app has one
    // (offered at most once per session per id; declines snooze via mcp::prefs).
    let mut offered_mcp: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    let mut last_mcp_offer_check = Instant::now();
    // By default the mic stays OPEN while the agent talks, so you can barge in and
    // interrupt its speech (native Live behaviour). On open speakers (no headphones
    // / no echo cancellation) the agent's own voice can leak into the mic and make
    // it interrupt itself - set CC_MIC_GATE=1 to mute the mic during playback.
    let echo_gate = std::env::var("CC_MIC_GATE").is_ok();

    while !stop.load(Ordering::SeqCst) {
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
            && !state.awaiting
            && state.has_command
            && last_event.elapsed() > Duration::from_secs(6) // genuinely idle (state.active sticks true → don't gate on it)
            && last_offer_check.elapsed() >= Duration::from_secs(4)
        {
            last_offer_check = Instant::now();
            if !super::browser::is_connected()
                && !super::browser::recently_connected()
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

        // 0c-mcp) proactive offer: the foreground app has a curated MCP integration that
        //     isn't installed (and hasn't been declined) → nudge the model to offer it once.
        if !state.awaiting
            && state.has_command
            && last_event.elapsed() > Duration::from_secs(6) // idle (not mid-response); state.active sticks true so don't gate on it
            && last_mcp_offer_check.elapsed() >= Duration::from_secs(4)
        {
            last_mcp_offer_check = Instant::now();
            let title = super::uia::pointer_context().0;
            if let Some(id) = super::mcp::detect_uninstalled_match(&title)
                && offered_mcp.insert(id)
                && let Some(name) = super::mcp::display_name(id)
            {
                let _ = send(
                    &mut socket,
                    realtime_text(&format!(
                        "(Heads-up for you, not the user: they're using {name}, which has a CURATED app-control \
integration giving you precise tools instead of clicking its UI. If it fits the moment, briefly offer ONCE - in \
their language - to set it up. They must say YES first (it installs + runs software), then call \
setup_app_integration with id:'{id}', confirmed:true. If they decline, call decline_app_integration with id:'{id}'.)"
                    )),
                );
                state.awaiting = true;
            }
        }

        // 0d) the server warned the session is ending (goAway). Reconnect PROACTIVELY
        //     at the next gap (no tool call in flight) so we migrate the conversation
        //     cleanly with our recap - instead of being force-closed mid-stream (which
        //     dropped us with a gap + a "client failed to close" error).
        if state.go_away && state.pending.id.is_none() {
            state.go_away = false;
            overlay::push_log("(goAway) reconnecting before the session ends".to_string());
            if !reconnect_session(
                &mut socket,
                &key,
                target.as_deref(),
                &mut reconnects,
                &mut state,
            )? {
                break;
            }
            last_event = Instant::now();
            last_frame = Instant::now();
            continue;
        }

        // 0e) an MCP integration connected/removed → the tool set changed. Gemini freezes
        //     tools at setup, so reconnect at the next safe gap to re-declare them. Clear the
        //     flag FIRST (no reconnect storm); skip while a tool call is in flight, the model
        //     is mid-think, or it's speaking (don't cut off its "done, it's ready").
        if super::mcp::tools_changed()
            && state.pending.id.is_none()
            && !state.awaiting
            && last_event.elapsed() > Duration::from_secs(2)
            && !sink.as_ref().map(|s| s.is_playing()).unwrap_or(false)
        {
            super::mcp::clear_tools_changed();
            overlay::push_log("(mcp) tools changed - reconnecting to activate".to_string());
            if !reconnect_session(
                &mut socket,
                &key,
                target.as_deref(),
                &mut reconnects,
                &mut state,
            )? {
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
        let playing = sink.as_ref().map(|s| s.is_playing()).unwrap_or(false);
        // Orb caption lifetime: the spoken reply keeps playing for many seconds AFTER its transcript
        // finishes, so clear it (and rest the orb) on REAL speech-end — not on a transcript-idle timer
        // (which made the text vanish mid-sentence). Debounced so a gap between audio chunks won't fire.
        if playing {
            speech_quiet = None;
        } else if was_playing {
            speech_quiet = Some(Instant::now());
        }
        was_playing = playing;
        if let Some(t) = speech_quiet
            && t.elapsed() > Duration::from_millis(1200)
        {
            overlay::set_model_idle();
            speech_quiet = None;
        }
        let muted = echo_gate && playing;
        let voiced = !chunk.is_empty() && !muted;
        // Drive the orb's VOLUME reaction every tick (0 when quiet) so the resting orb pulses with
        // your voice and settles the moment you stop. The visual gain is amplified in orb.html; the
        // Idle orb's reaction to this IS the "I hear you" feedback (no separate listening state).
        let level = if voiced {
            let rms = (chunk.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / chunk.len() as f64)
                .sqrt();
            (rms / 4000.0).min(1.0) as f32
        } else {
            0.0
        };
        overlay::set_orb_audio(level);
        // Speech ONSET (first audio after a gap), ONLY when the model isn't speaking. The model needs
        // a fresh frame to LEAD the turn: video + audio are concurrent streams with NO ordering
        // guarantee, and a frame sent at/after the turn closes isn't ingested in time — the model
        // receives no image (verified: 0s lead → "no image"; ≥0.5s lead → it reads the screen).
        // Pushing a frame the instant you start talking gives it the whole utterance (≥0.5s) to be
        // ingested before the turn.
        let onset = voiced && !playing && last_voice.elapsed() >= Duration::from_millis(500);
        if voiced {
            overlay::set_listening(true);
            send_audio_chunk(&mut socket, &chunk)?;
            last_voice = Instant::now();
        }

        // 2) send the model a fresh (cached) frame so it can SEE: immediately on speech onset (so a
        //    frame LEADS the turn), then at ~1 frame/FRAME_INTERVAL while you keep talking (3s tail) or
        //    a request is active. NOT while the model is speaking (wasteful input). The capturer stays
        //    warm while there's an active request or recent speech so the cache is fresh.
        capture_on.store(
            state.active || last_voice.elapsed() < Duration::from_secs(5),
            Ordering::SeqCst,
        );
        // Don't stream periodic frames while the model is mid-generation (`awaiting`) — they just
        // bloat the context it's already chewing on and slow the next decision. Onset (your speech)
        // and after-action frames still flow, so it never goes blind.
        let engaged = !playing
            && !state.awaiting
            && (state.active || last_voice.elapsed() < Duration::from_secs(3));
        if state.pending.id.is_none()
            && (onset || (engaged && last_frame.elapsed() >= FRAME_INTERVAL))
        {
            let f = frame_slot.lock().unwrap().clone();
            if let Some(f) = f {
                let _ = send(&mut socket, realtime_video_jpeg_b64(&f));
            }
            last_frame = Instant::now();
        }

        // 3) executor finished an action -> answer the tool (+ push the new frame).
        if let Ok((id, name, resp, frame)) = res_rx.try_recv()
            && state.pending.id.as_deref() == Some(id.as_str())
        {
            let mut reconnect_for_mcp_activation = false;
            if state.pending.cancelled {
                // The action finished (or was stopped); its result is dropped
                // because you spoke and the model already moved on.
                overlay::push_log("[~] step done; result dropped (you spoke)".to_string());
            } else {
                let resp_ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                reconnect_for_mcp_activation =
                    name == "app_integration_status" && activation_pending(&resp);
                record_observation(&mut state, &name, &resp); // durable memory of what we saw
                send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                if let Some(f) = frame {
                    let _ = send(&mut socket, realtime_video_jpeg_b64(&f)); // then frame
                }
                // An accepted `done` ends the request: go idle (stop pushing frames)
                // until the user speaks again. A rejected done keeps working.
                if name == "done" && resp_ok {
                    overlay::push_log("[done] goal reached".to_string());
                    overlay::set_orb_done();
                    state.active = false;
                    state.awaiting = false;
                } else if reconnect_for_mcp_activation {
                    state.awaiting = false;
                } else {
                    state.awaiting = true; // model owes the next action/turn
                    state.think_start = Some(Instant::now()); // measure the next think-time
                }
            }
            state.pending = Pending::default();
            cancel.store(false, Ordering::SeqCst);
            last_frame = Instant::now();
            // Restart the silence clock here: the model only "owes" us a response from
            // the moment we hand back the tool result, so a slow action (e.g. a 20s
            // vision look, or the stall planner) must NOT count as silence and trip a
            // false reconnect the instant it returns.
            last_event = Instant::now();
            state.nudged = false;
            overlay::set_status("ready - speak a command");
            if reconnect_for_mcp_activation {
                super::mcp::clear_tools_changed();
                overlay::push_log(
                    "(mcp) health passed - reconnecting now to activate tools".to_string(),
                );
                if !reconnect_session(
                    &mut socket,
                    &key,
                    target.as_deref(),
                    &mut reconnects,
                    &mut state,
                )? {
                    break;
                }
                last_frame = Instant::now();
                continue;
            }
        }

        // 3b) a typed command from the orb's text box → inject it as a user text turn, taking the
        // exact same path the spoken transcript does (so it drives the agent identically).
        if let Ok(cmd) = cmd_rx.try_recv() {
            let cmd = cmd.trim().to_string();
            if !cmd.is_empty() {
                let _ = send(&mut socket, realtime_text(&cmd));
                handle_event(
                    ServerEvent::InputTranscript(cmd),
                    sink.as_ref(),
                    &cancel,
                    &exec_tx,
                    &mut state,
                );
                last_event = Instant::now();
            }
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
                if !reconnect_session(
                    &mut socket,
                    &key,
                    target.as_deref(),
                    &mut reconnects,
                    &mut state,
                )? {
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
                        if !reconnect_session(
                            &mut socket,
                            &key,
                            target.as_deref(),
                            &mut reconnects,
                            &mut state,
                        )? {
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
                        if let Some(f) = frame_slot.lock().unwrap().clone() {
                            let _ = send(&mut socket, realtime_video_jpeg_b64(&f));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => {
                overlay::push_log(format!("read error: {e} - reconnecting"));
                if !reconnect_session(
                    &mut socket,
                    &key,
                    target.as_deref(),
                    &mut reconnects,
                    &mut state,
                )? {
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
    super::mcp::disconnect_all(); // kill MCP server children so none outlive the session
    Ok(())
}

fn activation_pending(resp: &Value) -> bool {
    resp.get("activation_pending")
        .and_then(Value::as_bool)
        .or_else(|| {
            resp.pointer("/action_result/activation_pending")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
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
        telemetry::typed_error(
            "ERR_SESSION_RECONNECT_LIMIT",
            "runtime",
            "session reconnect limit reached",
            serde_json::json!({"max_reconnects": MAX_RECONNECTS}),
        );
        return Ok(false);
    }
    overlay::set_status("reconnecting...");
    telemetry::event(
        "session_reconnect_start",
        "runtime",
        Privacy::Safe,
        serde_json::json!({"attempt": *reconnects, "active": state.active, "awaiting": state.awaiting}),
    );
    match uia_task::reconnect(key, None, true, false) {
        Ok(s) => *socket = s,
        Err(e) => {
            // A bad MCP tool schema can make setupComplete fail. Never brick the session:
            // suppress MCP tools and retry once so we always come back (just without them).
            overlay::push_log(format!(
                "reconnect failed: {e} - retrying without MCP tools"
            ));
            super::mcp::set_suppress_tools(true);
            match uia_task::reconnect(key, None, true, false) {
                Ok(s) => *socket = s,
                Err(e2) => {
                    overlay::push_log(format!("reconnect failed again: {e2}"));
                    return Ok(false);
                }
            }
        }
    }
    state.pending = Pending::default();
    state.nudged = false; // fresh session - re-arm the nudge
    flush_reply(state); // capture any in-flight reply before recapping
    if let Ok(f) = uia_task::snapshot(target) {
        send(socket, realtime_video_jpeg_b64(&f))?;
    }
    if !state.active {
        overlay::push_log("(reconnected - idle; waiting for user)".to_string());
        overlay::set_status("ready - speak a command");
        telemetry::event(
            "session_reconnect_idle",
            "runtime",
            Privacy::Safe,
            serde_json::json!({"attempt": *reconnects}),
        );
        return Ok(true);
    }
    let recap = build_recap(&state.history);
    // A reconnect is a SEAMLESS internal event, NOT a new user request. Re-establish
    // context, then let the model DECIDE whether to act or wait - it must not fire a
    // fresh (let alone consequential) action just because the socket reconnected.
    let judge = "JUDGE before doing anything: only finish a step if you were CLEARLY mid-way through an action the \
user already asked for AND the current screen is that task. Otherwise - task looks done, screen is unrelated, or \
you're unsure - take NO action and simply wait for the user (no narration needed). NEVER start a new or consequential \
action just because the connection reconnected.";
    let msg = if recap.is_empty() {
        format!("(reconnected seamlessly - not a new request) The current screen is shown. {judge}")
    } else {
        format!(
            "(reconnected seamlessly - not a new request) Our conversation so far, keep it as context:\n{recap}\n\nThe \
current screen is shown. {judge}"
        )
    };
    send(socket, realtime_text(&msg))?;
    overlay::push_log("(reconnected - conversation memory restored)".to_string());
    overlay::set_status("ready - speak a command");
    telemetry::event(
        "session_reconnect_reseeded",
        "runtime",
        Privacy::Safe,
        serde_json::json!({"attempt": *reconnects, "recap_chars": recap.chars().count()}),
    );
    Ok(true)
}

/// Best-effort: is the foreground window a web browser? (Brand names in window
/// titles are language-stable, e.g. "… - Google Chrome", "… - Microsoft Edge".)
fn foreground_is_browser() -> bool {
    let title = super::uia::pointer_context().0.to_lowercase();
    [
        "chrome", "edge", "brave", "opera", "firefox", "chromium", "vivaldi",
    ]
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
