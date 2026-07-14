//! Continuous Computer Control voice session with concurrent input, execution,
//! cancellation, grounding, and reconnect handling.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
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
mod cleanup;
mod control;
mod frames;
mod mic;
mod outcomes;
mod reader;
mod reader_policy;
mod reconnect_gate;
mod repeat_failure;
mod results;
mod scripted;
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
use results::{poll_action_result, send_immediate_tool_responses};
use session_control::{
    CatalogRecovery, activate_integrations, await_startup_catalog, configured_target,
    connect_initial_session, reconnect_session,
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
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    control::install_text_sender(cmd_tx);
    let exec_target = target.clone();
    let exec_stop = Arc::clone(stop);
    let exec_thread =
        std::thread::spawn(move || executor_loop(exec_target, exec_rx, res_tx, exec_stop));
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
    let mut catalog_recovery = CatalogRecovery::default();
    state.source_frame = initial_source;
    state.connection_generation = 1;
    cleanup.track_pending(&state);
    let mut reconnects = 0u32;
    // Keep a small set of distinct screens for searchable conversation memory.
    let mut last_mem_title = String::new();
    let mut last_mem_check = Instant::now();
    let mut scripted_turns: std::collections::VecDeque<String> =
        scripted_turns.unwrap_or_default().into();
    let scripted_started = Instant::now();
    let mut scripted_finished: Option<Instant> = None;
    let scripted_idle_settle = scripted::idle_settle();
    // Set CC_MIC_GATE=1 when speaker echo would otherwise self-interrupt replies.
    let echo_gate = std::env::var("CC_MIC_GATE").is_ok();
    while !stop.load(Ordering::SeqCst) {
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
        if scripted_mode {
            let deadline = scripted::deadline();
            if scripted_started.elapsed() > deadline {
                anyhow::bail!("scripted production run exceeded {}s", deadline.as_secs());
            }
            if state.pending.id.is_none()
                && !state.awaiting
                && !state.active
                && !sink.as_ref().is_some_and(AudioSink::is_playing)
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
                    user_audio_tracker.commit_transcript();
                    cleanup.track_pending(&state);
                    last_event = Instant::now();
                    scripted_finished = None;
                } else if scripted_finished.get_or_insert_with(Instant::now).elapsed()
                    > scripted_idle_settle
                {
                    if !scripted::has_accepted_completion(&state) {
                        anyhow::bail!("scripted turn became idle without an accepted completion");
                    }
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
                &mut catalog_recovery,
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
                &mut catalog_recovery,
                "go_away",
            )? {
                exit_reason = "go_away_reconnect_exhausted";
                break;
            }
            last_event = Instant::now();
            last_frame = Instant::now();
            continue;
        }

        let catalog_retry_due = catalog_recovery.retry_due(Instant::now());
        if (super::mcp::tools_changed() || catalog_retry_due)
            && reconnect_ready
            && last_event.elapsed() > Duration::from_secs(2)
        {
            if catalog_retry_due {
                catalog_recovery.begin_retry();
            }
            super::mcp::clear_tools_changed();
            let trigger = if catalog_retry_due {
                "bounded_catalog_retry"
            } else {
                "tool_catalog_changed"
            };
            overlay::push_log("(mcp) reconnecting to activate full catalog".to_string());
            if !reconnect_session(
                &mut socket,
                &key,
                target.as_deref(),
                &mut reconnects,
                &mut state,
                &mut catalog_recovery,
                trigger,
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
        user_audio_tracker.update(voiced, level, chunk.len());
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
            capture_cache_needed(state.active, state.terminal_drain, recent_voice),
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
                state.input_transcript.reset();
                handle_event(
                    ServerEvent::InputTranscript(cmd),
                    sink.as_ref(),
                    &exec_tx,
                    &mut state,
                );
                user_audio_tracker.commit_transcript();
                cleanup.track_pending(&state);
                last_event = Instant::now();
            }
        }

        if let Some(reconnect_for_mcp_activation) =
            poll_action_result(&mut socket, &res_rx, &mut state)?
        {
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
                    &mut catalog_recovery,
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
                            &mut catalog_recovery,
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
                    &mut catalog_recovery,
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
                user_audio_tracker.commit_transcript();
            }
            cleanup.track_pending(&state);
            if send_immediate_tool_responses(&mut socket, &mut state)? {
                last_event = Instant::now();
            }
        }
    }
    flush_reply(&mut state); // close the final spoken reply into the transcript
    emit_turn_summary(&mut state, "session_stop");
    super::memory::save(state.history.clone(), std::mem::take(&mut mem_frames));
    cleanup.finish(&mut state, exit_reason);
    Ok(())
}
