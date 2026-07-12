//! Continuous Computer Control voice session with concurrent input, execution,
//! cancellation, grounding, and reconnect handling.

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
mod control;
mod frames;
mod mic;
mod offers;
mod reader;
mod reader_policy;
mod results;
mod session_control;
mod speech_events;
mod speech_gate;
use action_worker::executor_loop;
pub(super) use control::{run, run_scripted, submit_text_command};
use frames::{capture_failed, send_snapshot};
use mic::{MicUplinkWindow, mic_thread};
use offers::Offers;
use reader::{
    Pending, Reader, build_recap, emit_turn_summary, flush_reply, handle_event, record_observation,
    record_tool_result,
};
use results::poll_action_result;
use session_control::{reconnect_session, record_session_end, wait_for_setup};
use speech_events::{PlaybackTracker, UserAudioTracker};

/// Frame cadence while talking/working; speech onset also pushes a leading frame.
const FRAME_INTERVAL: Duration = Duration::from_millis(1800);
const CAPTURE_CACHE_INTERVAL: Duration = Duration::from_millis(500);
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

/// A tool call handed to the executor thread. Each job owns its cancellation
/// token so a late result or a newer job can never resurrect cancelled work.
struct Job {
    id: String,
    name: String,
    args: Value,
    task: String,
    intent: String,
    user_text: String,
    action: telemetry::ActionTrace,
    source_frame_id: Option<u64>,
    queued_at: Instant,
    cancel: Arc<AtomicBool>,
}
/// A finished action from the executor: (id, name, response, optional frame b64).
type Done = (
    String,
    String,
    Value,
    Option<(String, u64)>,
    Arc<AtomicBool>,
    telemetry::ActionTrace,
);

fn run_inner(stop: &Arc<AtomicBool>, scripted_turns: Option<Vec<String>>) -> anyhow::Result<()> {
    let scripted_mode = scripted_turns.is_some();
    let key = session::load_key()?;
    let target = std::env::var("CC_UIA_WINDOW").ok();
    overlay::set_status("connecting...");

    // The mic owns its stream and device rebuilds on a dedicated thread so session/UIA COM state
    // cannot alter its apartment during a reconnect.
    let mic_buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let mic_pause = Arc::new(AtomicBool::new(false));
    {
        let buf = mic_buf.clone();
        let pause = mic_pause.clone();
        let mic_stop = Arc::clone(stop);
        std::thread::spawn(move || mic_thread(buf, pause, mic_stop));
    }
    let mut sink = AudioSink::new();
    let mut last_sink_recovery = Instant::now();
    if sink.is_none() {
        overlay::push_log("(no audio output device - replies shown as text only)".to_string());
        telemetry::typed_error(
            "ERR_AUDIO_OUTPUT_UNAVAILABLE",
            "speech",
            "no usable audio output device; replies will remain text-only",
            serde_json::json!({}),
        );
    }

    // Try WITH Google Search grounding; if setup is rejected (grounding needs a
    // billing-enabled project / quota), fall back to a search-less session so it
    // still starts. Other Live features don't use search, which is why they work.
    let mut socket = connect_ws(&key)?;
    let setup_with_search = uia_task::build_setup(None, true, true);
    telemetry::record_model_setup(&setup_with_search, "initial_search");
    send(&mut socket, setup_with_search)?;
    if wait_for_setup(&mut socket, stop).is_err() {
        let _ = socket.close(None);
        overlay::push_log(
            "(Google Search unavailable on this key; starting without it)".to_string(),
        );
        socket = connect_ws(&key)?;
        let setup_without_search = uia_task::build_setup(None, true, false);
        telemetry::record_model_setup(&setup_without_search, "initial_fallback");
        send(&mut socket, setup_without_search)?;
        wait_for_setup(&mut socket, stop)?;
    }
    set_socket_nonblocking(&mut socket)?;
    overlay::set_status("ready - speak a command");
    overlay::set_orb_resting();
    overlay::push_log(
        "* connected; sending your WHOLE screen + mic each turn (smart brain)".to_string(),
    );

    // Keep the executor off the socket thread so barge-in can halt slow actions.
    super::browser::ensure_started();
    // Bring any installed MCP app-control integrations back online (each on its own
    // thread, since a cold spawn can block); the reconnect-on-tools-changed gap then
    // re-runs build_setup to declare their tools.
    super::mcp::connect_all_installed();

    let (exec_tx, exec_rx) = mpsc::channel::<Job>();
    let (res_tx, res_rx) = mpsc::channel::<Done>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    control::install_text_sender(cmd_tx);
    let exec_target = target.clone();
    let exec_thread = std::thread::spawn(move || executor_loop(exec_target, exec_rx, res_tx));

    let f0 = match uia_task::snapshot(target.as_deref()) {
        Ok(frame) => {
            send_snapshot(&mut socket, &frame, "session_initial")?;
            Some(frame)
        }
        Err(error) => {
            capture_failed("session_initial", target.as_deref(), &error);
            None
        }
    };
    let initial_frame_id = f0.as_ref().map(|frame| frame.frame_id);

    // Capture into a latest-wins cache; synchronous capture would stall audio.
    let frame_slot: Arc<Mutex<Option<uia_task::SnapshotFrame>>> = Arc::new(Mutex::new(f0));
    let capture_on = Arc::new(AtomicBool::new(true));
    {
        let slot = frame_slot.clone();
        let on = capture_on.clone();
        let cap_stop = Arc::clone(stop);
        let cap_target = target.clone();
        std::thread::spawn(move || {
            let mut last_capture_error: Option<Instant> = None;
            while !cap_stop.load(Ordering::SeqCst) {
                if on.load(Ordering::SeqCst) {
                    match uia_task::snapshot(cap_target.as_deref()) {
                        Ok(frame) => *slot.lock().unwrap() = Some(frame),
                        Err(error) => {
                            if last_capture_error
                                .is_none_or(|last| last.elapsed() >= Duration::from_secs(10))
                            {
                                capture_failed("background_cache", cap_target.as_deref(), &error);
                                last_capture_error = Some(Instant::now());
                            }
                        }
                    }
                    std::thread::sleep(CAPTURE_CACHE_INTERVAL);
                } else {
                    std::thread::sleep(Duration::from_millis(120));
                }
            }
        });
    }

    let mut last_frame = Instant::now();
    let mut last_voice = Instant::now();
    let mut playback_tracker = PlaybackTracker::default();
    let mut user_audio_tracker = UserAudioTracker::default();
    let mut mic_uplink = MicUplinkWindow::new();
    let mut last_event = Instant::now();
    let mut state = Reader {
        source_frame_id: initial_frame_id,
        connection_generation: 1,
        ..Reader::default()
    };
    let mut reconnects = 0u32;
    // Keep a small set of distinct screens for searchable conversation memory.
    let mut mem_frames: Vec<Vec<u8>> = Vec::new();
    let mut last_mem_title = String::new();
    let mut last_mem_check = Instant::now();
    let mut offers = Offers::new();
    let mut scripted_turns: std::collections::VecDeque<String> =
        scripted_turns.unwrap_or_default().into();
    let scripted_started = Instant::now();
    let mut scripted_finished: Option<Instant> = None;
    // Set CC_MIC_GATE=1 when speaker echo would otherwise self-interrupt replies.
    let echo_gate = std::env::var("CC_MIC_GATE").is_ok();
    let mut exit_reason = "external_stop_flag";
    while !stop.load(Ordering::SeqCst) {
        let sink_failed = sink.as_ref().is_some_and(AudioSink::needs_rebuild);
        let retry_missing_sink = sink.is_none() && last_sink_recovery.elapsed() >= Duration::from_secs(2);
        if sink_failed || retry_missing_sink {
            let dropped_samples = sink.as_ref().map(AudioSink::queued_samples).unwrap_or(0);
            sink = AudioSink::new();
            last_sink_recovery = Instant::now();
            telemetry::event(
                "audio_output_rebuilt",
                "speech",
                Privacy::Safe,
                serde_json::json!({
                    "ok": sink.is_some(),
                    "dropped_output_samples": dropped_samples,
                }),
            );
            if sink.is_none() {
                overlay::push_log(
                    "(audio output recovery failed - replies remain text-only)".to_string(),
                );
            }
        }
        if scripted_mode {
            let deadline_secs = std::env::var("CC_SCRIPTED_DEADLINE_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(300);
            if scripted_started.elapsed() > Duration::from_secs(deadline_secs) {
                anyhow::bail!("scripted production run exceeded {deadline_secs}s");
            }
            if state.pending.id.is_none()
                && !state.awaiting
                && !state.active
                && !state.awaiting_done_boundary
            {
                if let Some(command) = scripted_turns.pop_front() {
                    telemetry::event(
                        "scripted_turn_injected",
                        "test_harness",
                        Privacy::UserText,
                        serde_json::json!({
                            "remaining_turns": scripted_turns.len(),
                            "command_preview": command.chars().take(240).collect::<String>(),
                        }),
                    );
                    send(&mut socket, realtime_text(&command))?;
                    handle_event(
                        ServerEvent::InputTranscript(command),
                        sink.as_ref(),
                        &exec_tx,
                        &mut state,
                    );
                    last_event = Instant::now();
                    scripted_finished = None;
                } else if scripted_finished.get_or_insert_with(Instant::now).elapsed()
                    > Duration::from_secs(2)
                {
                    exit_reason = "scripted_complete";
                    stop.store(true, Ordering::SeqCst);
                    continue;
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

        offers.poll(&mut socket, &mut state, last_event);

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
                "go_away",
            )? {
                exit_reason = "go_away_reconnect_exhausted";
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
                "tool_catalog_changed",
            )? {
                exit_reason = "tool_catalog_reconnect_exhausted";
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
        playback_tracker.update(playing, &mut state, sink.as_ref());
        let muted = scripted_mode || (echo_gate && playing);
        let rms = mic::rms(&chunk);
        let has_mic_audio = mic::should_upload(chunk.len(), muted);
        let voiced = has_mic_audio && rms >= 120.0;
        // Drive the orb's VOLUME reaction every tick (0 when quiet) so the resting orb pulses with
        // your voice and settles the moment you stop. The visual gain is amplified in orb.html; the
        // Idle orb's reaction to this IS the "I hear you" feedback (no separate listening state).
        let level = if voiced {
            (rms / 4000.0).min(1.0) as f32
        } else {
            0.0
        };
        user_audio_tracker.update(voiced, level, chunk.len());
        overlay::set_orb_audio(level);
        // Speech ONSET (first audio after a gap), ONLY when the model isn't speaking. The model needs
        // a fresh frame to LEAD the turn: video + audio are concurrent streams with NO ordering
        // guarantee, and a frame sent at/after the turn closes isn't ingested in time — the model
        // receives no image (verified: 0s lead → "no image"; ≥0.5s lead → it reads the screen).
        // Pushing a frame the instant you start talking gives it the whole utterance (≥0.5s) to be
        // ingested before the turn.
        let onset = voiced && !playing && last_voice.elapsed() >= Duration::from_millis(500);
        if has_mic_audio {
            send_audio_chunk(&mut socket, &chunk)?;
            mic_uplink.record(chunk.len(), voiced, rms);
        }
        if voiced {
            overlay::set_listening(true);
            last_voice = Instant::now();
        }
        mic_uplink.flush_if_due(muted);

        // 2) send the model a fresh (cached) frame so it can SEE: immediately on speech onset (so a
        //    frame LEADS the turn), then at ~1 frame/FRAME_INTERVAL while you keep talking (3s tail) or
        //    a request is active. NOT while the model is speaking (wasteful input). The capturer stays
        //    warm while there's an active request or recent speech so the cache is fresh.
        capture_on.store(
            state.active || last_voice.elapsed() < Duration::from_secs(5),
            Ordering::SeqCst,
        );
        // Periodic frames pause while the model is generating; onset/action frames remain.
        let engaged = !playing
            && !state.awaiting
            && (state.active || last_voice.elapsed() < Duration::from_secs(3));
        if state.pending.id.is_none()
            && (onset || (engaged && last_frame.elapsed() >= FRAME_INTERVAL))
        {
            let f = frame_slot.lock().unwrap().clone();
            if let Some(f) = f {
                let trigger = if onset {
                    "user_speech_onset"
                } else {
                    "periodic"
                };
                if send_snapshot(&mut socket, &f, trigger).is_ok() {
                    state.source_frame_id = Some(f.frame_id);
                }
            }
            last_frame = Instant::now();
        }

        // Replacement typed input wins over a just-finished result.
        if let Ok(cmd) = cmd_rx.try_recv() {
            let cmd = cmd.trim().to_string();
            if !cmd.is_empty() {
                let _ = send(&mut socket, realtime_text(&cmd));
                handle_event(
                    ServerEvent::InputTranscript(cmd),
                    sink.as_ref(),
                    &exec_tx,
                    &mut state,
                );
                last_event = Instant::now();
            }
        }

        if let Some(reconnect_for_mcp_activation) =
            poll_action_result(&mut socket, &res_rx, &mut state)?
        {
            last_frame = Instant::now();
            last_event = Instant::now();
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
                    "integration_activation",
                )? {
                    exit_reason = "integration_activation_reconnect_exhausted";
                    break;
                }
                last_frame = Instant::now();
                continue;
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
                    &format!("socket_close:{frame:?}"),
                )? {
                    exit_reason = "socket_close_reconnect_exhausted";
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
                if state.awaiting && state.pending.id.is_none() && !state.control_revoked {
                    let silent = last_event.elapsed();
                    if silent > RECONNECT_SILENCE {
                        overlay::push_log("(session still silent - reconnecting)".to_string());
                        if !reconnect_session(
                            &mut socket,
                            &key,
                            target.as_deref(),
                            &mut reconnects,
                            &mut state,
                            &format!("silence_timeout:{}ms", silent.as_millis()),
                        )? {
                            exit_reason = "silence_reconnect_exhausted";
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
                        overlay::set_status("still working...");
                        overlay::push_log("(nudging the model with a fresh frame)".to_string());
                        if let Some(f) = frame_slot.lock().unwrap().clone()
                            && send_snapshot(&mut socket, &f, "silence_nudge").is_ok()
                        {
                            state.source_frame_id = Some(f.frame_id);
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
                    &format!("read_error:{e}"),
                )? {
                    exit_reason = "read_error_reconnect_exhausted";
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
            handle_event(ev, sink.as_ref(), &exec_tx, &mut state);
        }
        while let Some(immediate) = state.immediate_tool_responses.pop_front() {
            let (id, name, response) = immediate;
            send(&mut socket, tool_response(&id, &name, response))?;
            telemetry::event(
                "immediate_tool_response_sent",
                "turn_policy",
                Privacy::Safe,
                serde_json::json!({
                    "tool_call_id": id,
                    "tool": name,
                    "turn_mode": state.turn_mode.as_str(),
                    "control_revoked": state.control_revoked,
                }),
            );
            last_event = Instant::now();
        }
        if state.control_revoked {
            state.control_nudge = None;
        } else if let Some(nudge) = state.control_nudge.take() {
            overlay::set_status("recovering...");
            let _ = send(&mut socket, realtime_text(&nudge));
            state.awaiting = true;
            state.think_start = Some(Instant::now());
            last_event = Instant::now();
        }
    }
    // Persist the session asynchronously for future memory lookup.
    flush_reply(&mut state); // close the final spoken reply into the transcript
    emit_turn_summary(&mut state, "session_stop");
    super::memory::save(state.history.clone(), std::mem::take(&mut mem_frames));

    // On stop, abort any in-flight action so the executor frees up promptly - else
    // join() blocks (up to a slow vision call) and the mic/audio client lingers,
    // accumulating across session restarts until WASAPI runs out of resources.
    state.pending.request_cancel();
    drop(exec_tx); // close the channel -> executor thread exits
    let _ = exec_thread.join();
    super::mcp::disconnect_all(); // kill MCP server children so none outlive the session
    record_session_end(&state, exit_reason);
    Ok(())
}
