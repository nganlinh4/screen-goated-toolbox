//! Worker thread for Gemini Live LLM connection pool.
//!
//! Each worker keeps ONE pre-warmed socket (connected + setup-complete) ready for
//! the most recently used (model, instruction). A request that matches reuses it
//! instantly — the connect+setup cost is paid ahead of time, off the request's
//! critical path — then the socket is discarded (single-use, so no conversation
//! context leaks between independent calls) and a fresh one is warmed for next.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tungstenite::Message;

use super::manager::GeminiLiveManager;
use super::types::LiveEvent;
use super::websocket::{
    is_setup_complete, parse_error, parse_live_response, send_live_content, send_live_setup,
    set_live_read_timeout,
};
use crate::APP;
use crate::api::realtime_audio::websocket::connect_websocket;

type LiveSocket = tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>;

/// Connect + run the setup handshake for `model`, returning a socket ready to
/// receive content. Used both on demand and to PRE-WARM the next socket.
fn connect_and_setup(
    api_key: &str,
    model: &str,
    instruction: Option<&str>,
    show_thinking: bool,
) -> anyhow::Result<LiveSocket> {
    let mut socket = connect_websocket(api_key)?;
    send_live_setup(&mut socket, model, instruction, show_thinking)?;
    let start = Instant::now();
    loop {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                if is_setup_complete(msg.as_str()) {
                    break;
                }
                if let Some(e) = parse_error(msg.as_str()) {
                    anyhow::bail!("{e}");
                }
            }
            Ok(Message::Binary(d)) => {
                if let Ok(t) = String::from_utf8(d.to_vec()) {
                    if is_setup_complete(&t) {
                        break;
                    }
                    if let Some(e) = parse_error(&t) {
                        anyhow::bail!("{e}");
                    }
                }
            }
            Ok(Message::Close(f)) => anyhow::bail!("closed during setup: {f:?}"),
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > Duration::from_secs(15) {
                    anyhow::bail!("setup timeout");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => anyhow::bail!("setup error: {e}"),
        }
    }
    set_live_read_timeout(&mut socket, Duration::from_millis(250))?;
    Ok(socket)
}

/// Stream one request over a ready socket: send the content, relay text chunks /
/// thinking / completion as `LiveEvent`s. The socket is consumed (closed by the
/// caller after). Returns Err only if the SOCKET itself failed before any content
/// (so a stale warm socket can be retried cold).
fn serve(
    socket: &mut LiveSocket,
    request: &super::types::QueuedLiveRequest,
    manager: &GeminiLiveManager,
) -> Result<(), ()> {
    if let Err(e) = send_live_content(socket, &request.req.content) {
        // No content was produced — signal the caller it can retry on a fresh socket.
        let _ = e;
        return Err(());
    }

    let mut thinking_sent = false;
    let mut content_started = false;
    let response_start = Instant::now();
    let response_timeout = Duration::from_secs(20);
    let idle_finalize_after = Duration::from_millis(1200);
    let mut last_content_at: Option<Instant> = None;

    loop {
        if !manager.is_generation_valid(request.generation) || manager.shutdown.load(Ordering::SeqCst) {
            break;
        }

        match socket.read() {
            Ok(Message::Text(msg)) => {
                let msg_str = msg.as_str();
                if let Some(error) = parse_error(msg_str) {
                    let _ = request.response_tx.send(LiveEvent::Error(error));
                    break;
                }
                let (text_chunk, is_thought, is_turn_complete) = parse_live_response(msg_str);
                if let Some(text) = text_chunk {
                    if is_thought {
                        if !thinking_sent && !content_started {
                            let _ = request.response_tx.send(LiveEvent::Thinking);
                            thinking_sent = true;
                        }
                    } else {
                        content_started = true;
                        last_content_at = Some(Instant::now());
                        let _ = request.response_tx.send(LiveEvent::TextChunk(text));
                    }
                }
                if is_turn_complete {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                    break;
                }
            }
            Ok(Message::Binary(data)) => {
                // Try to parse as JSON text; ignore raw audio (not UTF-8).
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    if let Some(error) = parse_error(&text) {
                        let _ = request.response_tx.send(LiveEvent::Error(error));
                        break;
                    }
                    let (text_chunk, is_thought, is_turn_complete) = parse_live_response(&text);
                    if let Some(chunk) = text_chunk {
                        if is_thought {
                            if !thinking_sent && !content_started {
                                let _ = request.response_tx.send(LiveEvent::Thinking);
                                thinking_sent = true;
                            }
                        } else {
                            content_started = true;
                            last_content_at = Some(Instant::now());
                            let _ = request.response_tx.send(LiveEvent::TextChunk(chunk));
                        }
                    }
                    if is_turn_complete {
                        let _ = request.response_tx.send(LiveEvent::TurnComplete);
                        break;
                    }
                }
            }
            Ok(Message::Close(frame)) => {
                if content_started {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                } else {
                    let detail = frame
                        .as_ref()
                        .map(|frame| {
                            if frame.reason.is_empty() {
                                format!("code {}", frame.code)
                            } else {
                                format!("code {}: {}", frame.code, frame.reason)
                            }
                        })
                        .unwrap_or_else(|| "no close details".to_string());
                    // No content yet + socket closed: retryable if this was a warm socket.
                    if response_start.elapsed() < Duration::from_millis(500) {
                        return Err(());
                    }
                    let _ = request.response_tx.send(LiveEvent::Error(format!(
                        "Connection closed before response content was received ({detail})"
                    )));
                }
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if let Some(last) = last_content_at
                    && last.elapsed() >= idle_finalize_after
                {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                    break;
                }
                if !content_started && response_start.elapsed() >= response_timeout {
                    let _ = request.response_tx.send(LiveEvent::Error("Response timeout".to_string()));
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                if !content_started && response_start.elapsed() < Duration::from_millis(500) {
                    return Err(()); // stale warm socket — let caller retry cold
                }
                let _ = request.response_tx.send(LiveEvent::Error(format!("Read error: {e}")));
                break;
            }
        }
    }
    Ok(())
}

/// Run a worker thread for the Gemini Live connection pool.
pub fn run_live_worker(manager: Arc<GeminiLiveManager>) {
    std::thread::sleep(Duration::from_millis(50)); // stagger startup
    // A single pre-warmed socket for the last (model, instruction) we served.
    let mut warm: Option<(LiveSocket, String, String)> = None;

    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }

        let queued_request = {
            let mut queue = manager.work_queue.lock().unwrap();
            while queue.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                queue = manager.work_signal.wait(queue).unwrap();
            }
            if manager.shutdown.load(Ordering::SeqCst) {
                return;
            }
            queue.pop_front()
        };
        let Some(request) = queued_request else {
            continue;
        };

        if !manager.is_generation_valid(request.generation) {
            let _ = request.response_tx.send(LiveEvent::Error("Request cancelled".to_string()));
            continue;
        }

        let api_key = match APP.lock() {
            Ok(app) => app.config.gemini_api_key.clone(),
            Err(_) => {
                let _ = request.response_tx.send(LiveEvent::Error("Failed to get config".to_string()));
                continue;
            }
        };
        if api_key.trim().is_empty() {
            let lang = APP.lock().ok().map(|a| a.config.ui_language.clone()).unwrap_or_else(|| "en".to_string());
            crate::overlay::utils::show_api_key_error_notification("NO_API_KEY:gemini", &lang);
            let _ = request.response_tx.send(LiveEvent::Error("NO_API_KEY:gemini".to_string()));
            continue;
        }

        let model = request.req.model.clone();
        let instruction = request.req.instruction.clone();
        let instr_opt = (!instruction.trim().is_empty()).then_some(instruction.as_str());

        // Reuse the pre-warmed socket if it matches this (model, instruction);
        // otherwise connect cold. On a stale warm socket, fall back to cold once.
        let warm_match = matches!(&warm, Some((_, m, i)) if *m == model && *i == instruction);
        let mut socket = if warm_match {
            warm.take().map(|(s, _, _)| s).unwrap()
        } else {
            warm = None; // wrong model/instruction — drop the warm one
            match connect_and_setup(&api_key, &model, instr_opt, request.req.show_thinking) {
                Ok(s) => s,
                Err(e) => {
                    let _ = request.response_tx.send(LiveEvent::Error(format!("Connection failed: {e}")));
                    continue;
                }
            }
        };

        if serve(&mut socket, &request, &manager).is_err() {
            // Warm socket was stale: reconnect cold and serve once more.
            let _ = socket.close(None);
            match connect_and_setup(&api_key, &model, instr_opt, request.req.show_thinking) {
                Ok(mut fresh) => {
                    let _ = serve(&mut fresh, &request, &manager);
                    let _ = fresh.close(None);
                }
                Err(e) => {
                    let _ = request.response_tx.send(LiveEvent::Error(format!("Connection failed: {e}")));
                }
            }
        } else {
            let _ = socket.close(None);
        }

        // Pre-warm the next socket during the gap before the next call — but ONLY
        // for the stateless, instruction-less vision case (CC fires these in a
        // loop). Skip it for instruction-bearing uses (e.g. the help assistant) so
        // we don't leave idle sockets open or warm a one-off call.
        if !manager.shutdown.load(Ordering::SeqCst) && instr_opt.is_none() {
            warm = connect_and_setup(&api_key, &model, None, request.req.show_thinking)
                .ok()
                .map(|s| (s, model, instruction));
        } else {
            warm = None;
        }
    }
}
