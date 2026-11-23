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
use super::protocol::{ServerEvent, realtime_text, realtime_video_jpeg_b64};
use super::session::{self, Sock, connect_ws, send};
use super::telemetry::{self, Privacy};
use super::uia_task;

mod action_worker;
mod action_worker_receive;
mod cleanup;
mod completion_responses;
pub(super) mod control;
mod dropped_receipt;
mod effect_reporting;
mod frame_cache;
mod frames;
mod input_activity;
mod job;
mod mic;
mod outcomes;
mod provider_input;
mod reader;
mod reader_policy;
mod reader_state;
mod reconciliation;
mod reconnect_gate;
mod repeat_failure;
mod response_telemetry;
mod results;
mod session_control;
mod session_memory;
mod speech_events;
mod terminal_drain;
mod text_turn;
mod tool_call_ids;
mod write_recovery;
use action_worker::executor_loop;
use cleanup::SessionCleanup;
pub(super) use control::{run, submit_text_command};
use frames::{capture_failed, send_snapshot};
use job::{
    CAPTURE_CACHE_INTERVAL, Done, FRAME_INTERVAL, Job, MAX_RECONNECTS, NUDGE_SILENCE,
    RECONNECT_SILENCE, RETIRE_TURN,
};
use mic::{MicUplinkWindow, mic_thread};
use outcomes::ToolOutcomeLedger;
use reader::{
    Pending, Reader, emit_turn_summary, flush_reply, handle_event, record_observation,
    record_tool_result,
};
use results::{poll_action_result, send_immediate_tool_responses};
use session_control::{
    activate_integrations, await_startup_catalog, configured_target, connect_initial_session,
    reconnect_session,
};
use speech_events::{PlaybackTracker, UserAudioTracker};
fn run_inner(stop: &Arc<AtomicBool>) -> anyhow::Result<()> {
    let _retention = super::session_retention::start_session();
    let mut state = Reader::default();
    let mut cleanup = SessionCleanup::new(Arc::clone(stop));
    let mut exit_reason = "external_stop_flag";
    super::browser::ensure_started();
    let key = session::load_key()?;
    session::check_connection_attempt(stop, None, None)?;
    let engine = super::engine::SessionGuard::start(stop);
    session::check_connection_attempt(stop, None, None)?;
    let _engine = engine?;
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
        let worker = std::thread::spawn(super::session_retention::inherit(move || {
            mic_thread(buf, pause, mic_stop)
        }));
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
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<control::TextCommand>(1);
    let _text_sender = control::install_text_sender(cmd_tx);
    let exec_target = target.clone();
    let exec_stop = Arc::clone(stop);
    let exec_thread = std::thread::spawn(super::session_retention::inherit(move || {
        executor_loop(exec_target, exec_rx, res_tx, cleanup_ack_tx, exec_stop)
    }));
    cleanup.register_executor(exec_tx.clone(), exec_thread);

    let mut initial_write_error = None;
    let f0 = match uia_task::snapshot(target.as_deref()) {
        Ok(frame) => match send_snapshot(&mut socket, &frame, "session_initial") {
            Ok(()) => Some(frame),
            Err(error) => {
                initial_write_error = Some(error);
                None
            }
        },
        Err(error) => {
            capture_failed("session_initial", target.as_deref(), &error);
            None
        }
    };
    let initial_source = f0.as_ref().map(|frame| frame.source.clone());

    let (frame_cache, worker) =
        frame_cache::FrameCache::start(f0, target.clone(), Arc::clone(stop));
    cleanup.register_worker("frame_capture", worker);
    let mut last_voice = Instant::now();
    let mut playback_tracker = PlaybackTracker::default();
    let mut user_audio_tracker = UserAudioTracker::default();
    let mut mic_uplink = MicUplinkWindow::new();
    let mut last_event = Instant::now();
    let mut provider_frames = provider_input::ProviderInput::default();
    let mut reconnect_deferred_for_voice = false;
    let mut activation_reconnect_pending = false;
    state.source_frame = initial_source;
    state.connection_generation = 1;
    state.connection_health.connected(Instant::now());
    cleanup.track_pending(&state);
    let mut reconnects = 0u32;
    let echo_gate = std::env::var("CC_MIC_GATE").is_ok();
    macro_rules! write_or_reconnect {
        ($label:lifetime, $result:expr, $trigger:literal) => {
            match $result {
                Ok(value) => value,
                Err(error) => {
                    if !write_recovery::recover_write(error, &mut socket, &mut reconnects, &mut state, write_recovery::ReconnectRequest { key: &key, target: target.as_deref(), stop, trigger: $trigger })? { exit_reason = "write_reconnect_exhausted"; break $label; }
                    last_event = Instant::now();
                    state.frame_delivery.defer_periodic();
                    continue $label;
                }
            }
        };
    }
    let run_result = (|| -> anyhow::Result<()> {
        'session: while !stop.load(Ordering::SeqCst) {
            reconciliation::discharge_if_quiet(&mut state, super::browser::effect_unsettled());
            if let Some(error) = initial_write_error.take() {
                write_or_reconnect!('session, Err::<(), _>(error), "initial_frame_write");
            }
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
                    stop,
                )? {
                    exit_reason = "integration_activation_reconnect_exhausted";
                    break;
                }
                last_event = Instant::now();
                state.frame_delivery.defer_periodic();
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
                    stop,
                    "go_away",
                )? {
                    exit_reason = "go_away_reconnect_exhausted";
                    break;
                }
                last_event = Instant::now();
                state.frame_delivery.defer_periodic();
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
                    stop,
                    "tool_catalog_changed",
                )? {
                    exit_reason = "tool_catalog_reconnect_exhausted";
                    break;
                }
                last_event = Instant::now();
                state.frame_delivery.defer_periodic();
                continue;
            }

            let chunk = {
                let mut b = mic_buf.lock().unwrap();
                std::mem::take(&mut *b)
            };
            let playing = sink.as_ref().map(|s| s.is_playing()).unwrap_or(false);
            playback_tracker.update(playing, &mut state, sink.as_ref());
            let muted = echo_gate && playing;
            let rms = mic::rms(&chunk);
            let has_mic_audio = mic::should_upload(chunk.len(), muted);
            let voiced = has_mic_audio && mic::is_voiced(&chunk);
            let level = if voiced {
                (rms / 4000.0).min(1.0) as f32
            } else {
                0.0
            };
            input_activity::observe(
                &mut user_audio_tracker,
                &mut state,
                voiced,
                level,
                chunk.len(),
                playing,
            );
            let _ = user_audio_tracker.report_missing_transcript();
            overlay::set_orb_audio(level);
            if voiced {
                overlay::set_listening(true);
                last_voice = Instant::now();
            }
            let activity = frame_cache::Activity::current(
                &state,
                playing,
                voiced,
                last_voice,
                user_audio_tracker.has_uncommitted_audio(),
            );
            let sent = frame_cache.send_due(&mut socket, &mut state, activity);
            let sent = if has_mic_audio {
                write_recovery::note_audio_gap(sent)
            } else {
                sent
            };
            write_or_reconnect!('session, sent, "frame_write");
            if has_mic_audio {
                let sent = send_audio_chunk(&mut socket, &chunk)
                    .map_err(|error| error.context(session::SocketWriteFailure));
                write_or_reconnect!('session, write_recovery::note_audio_gap(sent), "audio_write");
                mic_uplink.record(chunk.len(), voiced, rms);
            }
            mic_uplink.flush_if_due(muted);

            let text_turn_idle =
                text_turn::ready(&state, sink.as_ref().is_some_and(AudioSink::is_playing));
            control::set_turn_idle(text_turn_idle);
            if text_turn_idle && let Ok(cmd) = cmd_rx.try_recv() {
                control::mark_text_command_consumed();
                control::set_turn_idle(false);
                let cmd = cmd.into_text();
                if !cmd.is_empty() {
                    let sent = send(&mut socket, realtime_text(&cmd));
                    input_activity::begin_epoch(&mut state);
                    handle_event(
                        ServerEvent::InputTranscript(cmd),
                        sink.as_ref(),
                        &exec_tx,
                        &mut state,
                    );
                    user_audio_tracker.commit_transcript("local_text");
                    cleanup.track_pending(&state);
                    last_event = Instant::now();
                    write_or_reconnect!('session, sent, "text_write");
                }
            }

            let action_result =
                poll_action_result(&mut socket, &res_rx, &mut state, sink.as_ref(), stop);
            if let Some(reconnect_for_mcp_activation) =
                write_or_reconnect!('session, action_result, "action_response_write")
            {
                let sent = send_immediate_tool_responses(&mut socket, &mut state);
                write_or_reconnect!('session, sent, "policy_response_write");
                cleanup.track_pending(&state);
                state.frame_delivery.defer_periodic();
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
                        stop,
                        &format!("socket_close:{frame:?}"),
                    )? {
                        exit_reason = "socket_close_reconnect_exhausted";
                        break;
                    }
                    state.frame_delivery.defer_periodic();
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
                                stop,
                                &format!("silence_timeout:{}ms", silent.as_millis()),
                            )? {
                                exit_reason = "silence_reconnect_exhausted";
                                break;
                            }
                            last_event = Instant::now();
                            state.frame_delivery.defer_periodic();
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
                            state
                                .frame_delivery
                                .request("silence_nudge", Instant::now());
                            state.source_frame = None;
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
                        stop,
                        &format!("read_error:{e}"),
                    )? {
                        exit_reason = "read_error_reconnect_exhausted";
                        break;
                    }
                    state.frame_delivery.defer_periodic();
                    continue;
                }
            };
            let Some(events) = provider_frames.decode(&text)? else {
                continue 'session;
            };
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
                if tool_call_ids::take_queue_overflow(&mut state) {
                    // The rejected call can never be answered on this transport.
                    if !reconnect_session(
                        &mut socket,
                        &key,
                        target.as_deref(),
                        &mut reconnects,
                        &mut state,
                        stop,
                        "tool_response_queue_overflow",
                    )? {
                        exit_reason = "queue_overflow_reconnect_exhausted";
                        break 'session;
                    }
                    last_event = Instant::now();
                    state.frame_delivery.defer_periodic();
                    continue 'session;
                }
                cleanup.track_pending(&state);
                let sent = send_immediate_tool_responses(&mut socket, &mut state);
                if write_or_reconnect!('session, sent, "immediate_response_write") {
                    last_event = Instant::now();
                }
            }
        }
        Ok(())
    })();
    provider_frames.report_summary();
    results::record_executor_disconnect_reason(&run_result, &mut exit_reason);
    speech_events::discard_generation_audio(&mut state, "session_stopped");
    flush_reply(&mut state); // close the final spoken reply into the transcript
    emit_turn_summary(&mut state, "session_stop");
    std::mem::take(&mut state.memory).finish(state.history.clone());
    cleanup.finish(&mut state, exit_reason);
    run_result
}
