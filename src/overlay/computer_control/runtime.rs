//! Continuous Computer Control session with concurrent input, actions, grounding, and reconnects.

use serde_json::Value;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tungstenite::Message;

use crate::api::gemini_live::transport::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};
use crate::api::realtime_audio::websocket::send_audio_chunk;

use super::overlay;
use super::playback::AudioSink;
use super::protocol::{
    ServerEvent, parse_server_message, realtime_text, realtime_video_jpeg_b64, tool_response,
};
use super::session::{self, Sock, connect_ws, send};
use super::telemetry::{self, Privacy};
use super::uia_task;

mod action_worker;
mod action_worker_receive;
mod cleanup;
mod completion_responses;
mod control;
mod effect_reporting;
mod frames;
mod mic;
mod outcomes;
mod reader;
mod reader_policy;
mod reader_state;
mod reconnect_gate;
mod repeat_failure;
mod response_telemetry;
mod results;
mod scripted;
#[cfg(any(debug_assertions, test))]
mod scripted_snapshots;
mod session_control;
mod speech_events;
mod terminal_drain;
use action_worker::executor_loop;
use cleanup::SessionCleanup;
pub(super) use control::{run, run_scripted, submit_text_command};
use frames::{capture_cache_needed, capture_failed, send_snapshot};
use mic::{MicUplinkWindow, mic_thread};
use outcomes::ToolOutcomeLedger;
use reader::{
    Pending, Reader, emit_turn_summary, flush_reply, handle_event, record_observation,
    record_tool_result,
};
/// One-way worker cleanup when the model ends a turn without calling `done`.
/// This is never exposed to the model and never produces a model response.
const RETIRE_TURN: &str = "__retire_turn__";
use results::{poll_action_result, send_immediate_tool_responses};
use session_control::{
    activate_integrations, await_startup_catalog, configured_target, connect_initial_session,
    reconnect_session,
};
use speech_events::{PlaybackTracker, UserAudioTracker};
/// Frame cadence while talking/working; speech onset also pushes a leading frame.
const FRAME_INTERVAL: Duration = Duration::from_millis(1800);
const CAPTURE_CACHE_INTERVAL: Duration = Duration::from_millis(500);
const MAX_RECONNECTS: u32 = 6;
/// Silence recovery nudges once, then reconnects only above healthy turn latency;
/// reconnecting abandons the current generation.
const NUDGE_SILENCE: Duration = Duration::from_secs(8);
const RECONNECT_SILENCE: Duration = Duration::from_secs(40);

/// A tool call handed to the executor thread. Each job owns its cancellation
/// token so a late result or a newer job can never resurrect cancelled work.
struct Job {
    id: String,
    name: String,
    args: Value,
    task: String,
    user_text: String,
    inherit_evidence: bool,
    action: telemetry::ActionTrace,
    source_frame: Option<uia_task::FrameSource>,
    queued_at: Instant,
    cancel: Arc<AtomicBool>,
}
/// A finished action from the executor: (id, name, response, optional frame b64).
type Done = (
    String,
    String,
    Value,
    Option<(String, uia_task::FrameSource)>,
    Arc<AtomicBool>,
    telemetry::ActionTrace,
);

fn run_inner(stop: &Arc<AtomicBool>, scripted_turns: Option<Vec<String>>) -> anyhow::Result<()> {
    let mut state = Reader::default();
    let mut cleanup = SessionCleanup::new(Arc::clone(stop));
    let mut mem_frames: Vec<Vec<u8>> = Vec::new();
    let mut exit_reason = "external_stop_flag";
    let scripted_mode = scripted_turns.is_some();
    let mut scripted_driver = scripted_turns
        .map(scripted::ScriptedDriver::new)
        .transpose()?;
    super::browser::ensure_started();
    let key = session::load_key()?;
    let target = configured_target()?;
    overlay::set_status("connecting...");

    // The mic owns its stream and device rebuilds on a dedicated thread so session/UIA COM state
    // cannot alter its apartment during a reconnect.
    let mic_buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let mic_pause = Arc::new(AtomicBool::new(false));
    {
        let buf = mic_buf.clone();
        let pause = mic_pause.clone();
        let mic_stop = Arc::clone(stop);
        let worker = std::thread::spawn(move || mic_thread(buf, pause, mic_stop));
        cleanup.register_worker("microphone", worker);
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

    let startup_catalog = super::mcp::connect_all_installed(Arc::clone(stop));
    cleanup.mark_mcp_started();
    await_startup_catalog(startup_catalog, stop)?;

    let mut socket = connect_initial_session(&key, stop)?;
    if !super::browser::await_startup_readiness(stop) {
        return Ok(());
    }
    overlay::set_status("ready - speak a command");
    overlay::set_orb_resting();
    overlay::push_log(
        "* connected; sending your WHOLE screen + mic each turn (smart brain)".to_string(),
    );

    let (exec_tx, exec_rx) = mpsc::channel::<Job>();
    let (res_tx, res_rx) = mpsc::channel::<Done>();
    let (cleanup_ack_tx, cleanup_ack_rx) = mpsc::channel::<u64>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    control::install_text_sender(cmd_tx);
    let exec_target = target.clone();
    let exec_stop = Arc::clone(stop);
    let exec_thread = std::thread::spawn(move || {
        executor_loop(exec_target, exec_rx, res_tx, cleanup_ack_tx, exec_stop)
    });
    cleanup.register_executor(exec_tx.clone(), exec_thread);

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
    let initial_source = f0.as_ref().map(|frame| frame.source.clone());

    // Capture into a latest-wins cache; synchronous capture would stall audio.
    let frame_slot: Arc<Mutex<Option<uia_task::SnapshotFrame>>> = Arc::new(Mutex::new(f0));
    let capture_on = Arc::new(AtomicBool::new(true));
    {
        let slot = frame_slot.clone();
        let on = capture_on.clone();
        let cap_stop = Arc::clone(stop);
        let cap_target = target.clone();
        let worker = std::thread::spawn(move || {
            let mut failure_gate = frames::CaptureFailureGate::new();
            while !cap_stop.load(Ordering::SeqCst) {
                if on.load(Ordering::SeqCst) {
                    match uia_task::snapshot(cap_target.as_deref()) {
                        Ok(frame) => {
                            *slot.lock().unwrap() = Some(frame);
                            failure_gate.on_success();
                        }
                        Err(error) => {
                            if failure_gate.on_failure(Instant::now()) {
                                capture_failed("background_cache", cap_target.as_deref(), &error);
                            }
                        }
                    }
                    std::thread::sleep(CAPTURE_CACHE_INTERVAL);
                } else {
                    failure_gate.on_success();
                    std::thread::sleep(Duration::from_millis(120));
                }
            }
        });
        cleanup.register_worker("frame_capture", worker);
    }

    let mut last_frame = Instant::now();
    let mut last_voice = Instant::now();
    let mut playback_tracker = PlaybackTracker::default();
    let mut user_audio_tracker = UserAudioTracker::default();
    let mut mic_uplink = MicUplinkWindow::new();
    let mut last_event = Instant::now();
    let mut reconnect_deferred_for_voice = false;
    let mut activation_reconnect_pending = false;
    state.source_frame = initial_source;
    state.connection_generation = 1;
    cleanup.track_pending(&state);
    let mut reconnects = 0u32;
    // Keep a small set of distinct screens for searchable conversation memory.
    let mut last_mem_title = String::new();
    let mut last_mem_check = Instant::now();
    // Set CC_MIC_GATE=1 when speaker echo would otherwise self-interrupt replies.
    let echo_gate = std::env::var("CC_MIC_GATE").is_ok();
    while !stop.load(Ordering::SeqCst) {
        cleanup::drain_turn_cleanup_acks(&cleanup_ack_rx, &mut state);
        let sink_failed = sink.as_ref().is_some_and(AudioSink::needs_rebuild);
        let retry_missing_sink =
            sink.is_none() && last_sink_recovery.elapsed() >= Duration::from_secs(2);
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
        if let Some(driver) = scripted_driver.as_mut() {
            let idle = scripted::runtime_idle(
                &state,
                sink.as_ref().is_some_and(AudioSink::is_playing),
                super::mcp::tools_changed() || state.go_away || activation_reconnect_pending,
            );
            match driver.step(&state, idle)? {
                scripted::ScriptedStep::Inject(command) => {
                    send(&mut socket, realtime_text(&command))?;
                    state.input_transcript.begin_epoch();
                    handle_event(
                        ServerEvent::InputTranscript(command),
                        sink.as_ref(),
                        &exec_tx,
                        &mut state,
                    );
                    user_audio_tracker.commit_transcript("scripted_text");
                    cleanup.track_pending(&state);
                    last_event = Instant::now();
                }
                scripted::ScriptedStep::Complete => {
                    exit_reason = "scripted_complete";
                    stop.store(true, Ordering::SeqCst);
                    continue;
                }
                scripted::ScriptedStep::Wait => {}
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

        let user_is_speaking = reconnect_gate::user_audio_active(
            &mic_buf,
            last_voice,
            user_audio_tracker.has_uncommitted_audio(),
        );
        reconnect_gate::record_catalog_deferral(
            user_is_speaking,
            &mut reconnect_deferred_for_voice,
        );
        let reconnect_ready = reconnect_gate::intentional_reconnect_ready(
            &state,
            user_is_speaking,
            sink.as_ref().is_some_and(AudioSink::is_playing),
        );

        if activation_reconnect_pending && reconnect_ready {
            activation_reconnect_pending = false;
            if !activate_integrations(
                &mut socket,
                &key,
                target.as_deref(),
                &mut reconnects,
                &mut state,
            )? {
                exit_reason = "integration_activation_reconnect_exhausted";
                break;
            }
            last_event = Instant::now();
            last_frame = Instant::now();
            continue;
        }

        if state.go_away && reconnect_ready {
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

        if super::mcp::tools_changed()
            && reconnect_ready
            && last_event.elapsed() > Duration::from_secs(2)
        {
            super::mcp::clear_tools_changed();
            overlay::push_log("(mcp) reconnecting to activate full catalog".to_string());
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

        let chunk = {
            let mut b = mic_buf.lock().unwrap();
            std::mem::take(&mut *b)
        };
        let playing = sink.as_ref().map(|s| s.is_playing()).unwrap_or(false);
        playback_tracker.update(playing, &mut state, sink.as_ref());
        let muted = scripted_mode || (echo_gate && playing);
        let rms = mic::rms(&chunk);
        let has_mic_audio = mic::should_upload(chunk.len(), muted);
        let voiced = has_mic_audio && mic::is_voiced(&chunk);
        // Drive the orb's volume reaction every tick so it tracks speech.
        let level = if voiced {
            (rms / 4000.0).min(1.0) as f32
        } else {
            0.0
        };
        if user_audio_tracker.update_for_local_epoch(voiced, level, chunk.len(), playing) {
            state.input_transcript.begin_epoch();
        }
        let _ = user_audio_tracker.report_missing_transcript();
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
        let recent_voice = last_voice.elapsed() < Duration::from_secs(5);
        capture_on.store(
            capture_cache_needed(
                state.active,
                state.terminal_drain,
                recent_voice,
                state.pending.id.is_some(),
                voiced,
            ),
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
                    state.source_frame = Some(f.source.clone());
                }
            }
            last_frame = Instant::now();
        }

        if let Ok(cmd) = cmd_rx.try_recv() {
            let cmd = cmd.trim().to_string();
            if !cmd.is_empty() {
                let _ = send(&mut socket, realtime_text(&cmd));
                state.input_transcript.begin_epoch();
                handle_event(
                    ServerEvent::InputTranscript(cmd),
                    sink.as_ref(),
                    &exec_tx,
                    &mut state,
                );
                user_audio_tracker.commit_transcript("local_text");
                cleanup.track_pending(&state);
                last_event = Instant::now();
            }
        }

        if let Some(reconnect_for_mcp_activation) =
            poll_action_result(&mut socket, &res_rx, &mut state, sink.as_ref())?
        {
            send_immediate_tool_responses(&mut socket, &mut state)?;
            cleanup.track_pending(&state);
            last_frame = Instant::now();
            last_event = Instant::now();
            if reconnect_for_mcp_activation {
                activation_reconnect_pending = true;
                let audio_active_now = reconnect_gate::user_audio_active(
                    &mic_buf,
                    last_voice,
                    user_audio_tracker.has_uncommitted_audio(),
                );
                reconnect_gate::record_activation_deferral(audio_active_now);
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
                // vision call). Recover gently first with a fresh ambient frame;
                // only if it stays silent do we replace the transport.
                if reader_policy::recovery_due(&state) {
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
                            state.source_frame = Some(f.source.clone());
                        }
                    }
                }
                terminal_drain::expire_after_socket_drained(&mut state, sink.as_ref());
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
        reconnects = 0; // healthy transport read - reset the reconnect budget
        let events = parse_server_message(&text);
        if events.iter().any(reconnect_gate::generation_progress) {
            last_event = Instant::now();
            state.nudged = false;
        }
        for ev in events {
            let commits_user_audio =
                matches!(&ev, ServerEvent::InputTranscript(text) if !text.trim().is_empty());
            handle_event(ev, sink.as_ref(), &exec_tx, &mut state);
            if commits_user_audio {
                user_audio_tracker.commit_transcript("provider_input_transcript");
            }
            cleanup.track_pending(&state);
            if send_immediate_tool_responses(&mut socket, &mut state)? {
                last_event = Instant::now();
            }
        }
    }
    speech_events::discard_generation_audio(&mut state, "session_stopped");
    flush_reply(&mut state); // close the final spoken reply into the transcript
    emit_turn_summary(&mut state, "session_stop");
    if !scripted_mode {
        super::memory::save(state.history.clone(), std::mem::take(&mut mem_frames));
    }
    cleanup.finish(&mut state, exit_reason);
    Ok(())
}
