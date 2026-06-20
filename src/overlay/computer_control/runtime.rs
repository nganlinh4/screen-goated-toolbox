//! Continuous Computer Control session: connect, stream mic + screen, dispatch
//! the model's tool calls to the OS executor, and surface progress to the
//! overlay. Single-threaded socket loop (read + write + execute) — executor
//! actions are fast, so the synchronous-tool-call deadlock does not bite here.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tungstenite::Message;

use crate::api::realtime_audio::websocket::{
    is_transient_socket_read_error, send_audio_chunk, set_socket_nonblocking,
    set_socket_short_timeout,
};

use super::executor;
use super::human_input::HumanProfile;
use super::overlay;
use super::playback::AudioSink;
use super::protocol::{
    self, ServerEvent, build_runtime_setup, parse_server_message, realtime_video_jpeg_b64,
    tool_response,
};
use super::session::{self, Sock, capture_frame, connect_ws, send};

/// How often a fresh screenshot is pushed during the session.
const FRAME_INTERVAL: Duration = Duration::from_millis(1800);

pub(super) fn run(stop: Arc<AtomicBool>) {
    match run_inner(&stop) {
        Ok(()) => overlay::set_status("stopped"),
        Err(e) => {
            overlay::push_log(format!("⚠ session error: {e}"));
            overlay::set_status("error");
        }
    }
    overlay::set_listening(false);
}

fn run_inner(stop: &Arc<AtomicBool>) -> anyhow::Result<()> {
    let key = session::load_key()?;
    overlay::set_status("connecting…");
    let mut socket = connect_ws(&key)?;
    send(
        &mut socket,
        build_runtime_setup(protocol::MODEL, protocol::RUNTIME_SYSTEM_INSTRUCTION),
    )?;
    wait_for_setup(&mut socket, stop)?;
    set_socket_nonblocking(&mut socket)?;
    overlay::set_status("ready — speak a command");
    overlay::push_log("● connected; streaming screen + mic".to_string());

    // Stream the microphone (16 kHz mono i16) into a shared buffer.
    let mic_buf: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let mic_pause = Arc::new(AtomicBool::new(false));
    let _mic_stream = crate::api::realtime_audio::start_mic_capture(
        mic_buf.clone(),
        stop.clone(),
        mic_pause,
    )?; // kept alive for the session

    // Output voice (24 kHz). Optional — if no output device, run muted.
    let sink = AudioSink::new();
    if sink.is_none() {
        overlay::push_log("(no audio output device — replies shown as text only)".to_string());
    }

    // Steer/stop core: actions run on a SEPARATE thread so the reader keeps
    // receiving mic + barge-in events WHILE a (possibly slow, humanized) action
    // runs. CANCEL is flipped on barge-in; the executor polls it between
    // micro-steps so a spoken "stop" halts SendInput mid-glide. Because 3.1's
    // function calling is synchronous, the model is blocked awaiting our
    // toolResponse — so we ALWAYS answer the pending id (unless the server itself
    // cancelled it) or the session deadlocks.
    let profile = HumanProfile::from_env();
    let cancel = Arc::new(AtomicBool::new(false));
    let (exec_tx, exec_rx) = mpsc::channel::<(String, String, serde_json::Value)>();
    let (res_tx, res_rx) = mpsc::channel::<(String, String, serde_json::Value)>();
    let exec_cancel = cancel.clone();
    let exec_thread = std::thread::spawn(move || {
        while let Ok((id, name, args)) = exec_rx.recv() {
            exec_cancel.store(false, Ordering::SeqCst); // each action starts fresh
            let result = executor::execute_ex(&name, &args, &profile, &exec_cancel);
            if res_tx.send((id, name, result)).is_err() {
                break;
            }
        }
    });

    let (frame0, _geom) = capture_frame()?;
    send(&mut socket, realtime_video_jpeg_b64(&frame0))?;
    let mut last_frame = Instant::now();
    let mut pending = Pending::default();

    while !stop.load(Ordering::SeqCst) {
        // 1) mic -> server, GATED while our own TTS is playing (so the agent's
        //    voice doesn't trip barge-in on itself).
        let chunk = {
            let mut b = mic_buf.lock().unwrap();
            std::mem::take(&mut *b)
        };
        let speaking = sink.as_ref().map(|s| s.is_playing()).unwrap_or(false);
        if !chunk.is_empty() && !speaking {
            overlay::set_listening(true);
            send_audio_chunk(&mut socket, &chunk)?;
        }

        // 2) periodic frame only while idle (a blocked model ignores mid-action frames).
        if pending.id.is_none() && last_frame.elapsed() >= FRAME_INTERVAL {
            if let Ok((f, _g)) = capture_frame() {
                let _ = send(&mut socket, realtime_video_jpeg_b64(&f));
            }
            last_frame = Instant::now();
        }

        // 3) executor finished an action -> answer the tool + re-ground.
        if let Ok((id, name, result)) = res_rx.try_recv()
            && pending.id.as_deref() == Some(id.as_str())
        {
            if pending.cancelled {
                overlay::push_log("✕ action cancelled by you".to_string());
            } else {
                send(&mut socket, tool_response(&id, &name, result))?;
            }
            pending = Pending::default();
            cancel.store(false, Ordering::SeqCst);
            if let Ok((f, _g)) = capture_frame() {
                let _ = send(&mut socket, realtime_video_jpeg_b64(&f));
            }
            last_frame = Instant::now();
            overlay::set_status("ready — speak a command");
        }

        // 4) read one event.
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(frame)) => {
                overlay::push_log(format!("socket closed: {frame:?}"));
                break;
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
                overlay::push_log(format!("read error: {e}"));
                break;
            }
        };
        for ev in parse_server_message(&text) {
            handle_event(&mut socket, ev, sink.as_ref(), &cancel, &exec_tx, &mut pending)?;
        }
    }
    drop(exec_tx); // close the channel -> executor thread exits
    let _ = exec_thread.join();
    Ok(())
}

/// The single in-flight tool call (synchronous FC ⇒ at most one), plus whether
/// the server cancelled it (in which case we must NOT answer it).
#[derive(Default)]
struct Pending {
    id: Option<String>,
    cancelled: bool,
}

fn handle_event(
    socket: &mut Sock,
    ev: ServerEvent,
    sink: Option<&AudioSink>,
    cancel: &Arc<AtomicBool>,
    exec_tx: &mpsc::Sender<(String, String, serde_json::Value)>,
    pending: &mut Pending,
) -> anyhow::Result<()> {
    match ev {
        ServerEvent::Audio(pcm) => {
            if let Some(sink) = sink {
                sink.push(&pcm);
            }
        }
        ServerEvent::Interrupted => {
            // Barge-in: stop talking immediately, and if an action is mid-flight,
            // halt it (the user spoke — listen, don't keep clicking).
            if let Some(sink) = sink {
                sink.clear();
            }
            if pending.id.is_some() {
                cancel.store(true, Ordering::SeqCst);
                overlay::set_status("halting…");
                overlay::push_log("⏸ you spoke — halting".to_string());
            }
        }
        ServerEvent::ToolCancellation(ids) => {
            if let Some(sink) = sink {
                sink.clear();
            }
            if let Some(p) = pending.id.as_ref()
                && ids.iter().any(|i| i == p)
            {
                pending.cancelled = true; // server discarded it — don't answer
            }
            cancel.store(true, Ordering::SeqCst);
            overlay::push_log(format!("✕ cancelled {ids:?}"));
        }
        ServerEvent::InputTranscript(t) => {
            // Local fast-path: a spoken stop halts NOW, before the round-trip.
            let lt = t.to_lowercase();
            if pending.id.is_some()
                && (lt.contains("stop") || lt.contains("dừng") || lt.contains("wait"))
            {
                cancel.store(true, Ordering::SeqCst);
                overlay::set_status("halting…");
            }
            overlay::set_user_text(t);
            overlay::set_listening(false);
        }
        ServerEvent::ModelText(t) | ServerEvent::OutputTranscript(t) => overlay::set_model_text(t),
        ServerEvent::ToolCall { id, name, args } => {
            if name == "done" {
                let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                overlay::push_log(format!("✓ done: {summary}"));
                overlay::set_status("ready — speak a command");
                send(socket, tool_response(&id, &name, serde_json::json!({"ok": true})))?;
                return Ok(());
            }
            overlay::push_log(format!("▸ {name} {}", compact_args(&args)));
            overlay::set_status(format!("doing: {name}"));
            *pending = Pending { id: Some(id.clone()), cancelled: false };
            let _ = exec_tx.send((id, name, args)); // runs on the executor thread
        }
        ServerEvent::GoAway { time_left } => {
            overlay::push_log(format!("server goAway ({time_left}) — session will end"))
        }
        _ => {}
    }
    Ok(())
}

fn compact_args(args: &serde_json::Value) -> String {
    let s = args.to_string();
    let clipped: String = s.chars().take(80).collect();
    if clipped.len() < s.len() {
        format!("{clipped}…")
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
