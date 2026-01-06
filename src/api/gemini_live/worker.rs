//! Worker thread for Gemini Live LLM connection pool

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::Message;

use super::manager::GeminiLiveManager;
use super::types::LiveEvent;
use super::websocket::{
    connect_live_websocket, is_setup_complete, parse_error, parse_live_response, send_live_content,
    send_live_setup,
};
use crate::APP;

/// Run a worker thread for the Gemini Live connection pool
/// Each worker handles one request at a time with its own WebSocket connection
pub fn run_live_worker(manager: Arc<GeminiLiveManager>) {
    // Stagger worker startup
    std::thread::sleep(Duration::from_millis(50));

    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }

        // Wait for a request
        let queued_request = {
            let mut queue = manager.work_queue.lock().unwrap();
            while queue.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                let result = manager.work_signal.wait(queue).unwrap();
                queue = result;
            }
            if manager.shutdown.load(Ordering::SeqCst) {
                return;
            }
            queue.pop_front()
        };

        let Some(request) = queued_request else {
            continue;
        };

        // Check if request is stale (interrupted)
        if !manager.is_generation_valid(request.generation) {
            let _ = request
                .response_tx
                .send(LiveEvent::Error("Request cancelled".to_string()));
            continue;
        }

        // Get API key
        let api_key = match APP.lock() {
            Ok(app) => app.config.gemini_api_key.clone(),
            Err(_) => {
                let _ = request
                    .response_tx
                    .send(LiveEvent::Error("Failed to get config".to_string()));
                continue;
            }
        };

        if api_key.trim().is_empty() {
            let _ = request
                .response_tx
                .send(LiveEvent::Error("NO_API_KEY:gemini".to_string()));
            continue;
        }

        // Connect to WebSocket
        let socket_result = connect_live_websocket(&api_key);
        let mut socket = match socket_result {
            Ok(s) => s,
            Err(e) => {
                let _ = request
                    .response_tx
                    .send(LiveEvent::Error(format!("Connection failed: {}", e)));
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };

        // Send setup message
        let instruction = if request.req.instruction.trim().is_empty() {
            None
        } else {
            Some(request.req.instruction.as_str())
        };

        if let Err(e) = send_live_setup(&mut socket, instruction, request.req.show_thinking) {
            let _ = request
                .response_tx
                .send(LiveEvent::Error(format!("Setup failed: {}", e)));
            let _ = socket.close(None);
            continue;
        }

        // Wait for setup acknowledgment
        let setup_start = Instant::now();
        let mut setup_complete = false;

        loop {
            if !manager.is_generation_valid(request.generation)
                || manager.shutdown.load(Ordering::SeqCst)
            {
                let _ = socket.close(None);
                let _ = request
                    .response_tx
                    .send(LiveEvent::Error("Cancelled".to_string()));
                break;
            }

            match socket.read() {
                Ok(Message::Text(msg)) => {
                    let msg_str = msg.as_str();
                    if is_setup_complete(msg_str) {
                        setup_complete = true;
                        break;
                    }
                    if let Some(error) = parse_error(msg_str) {
                        let _ = request.response_tx.send(LiveEvent::Error(error));
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                        if is_setup_complete(&text) {
                            setup_complete = true;
                            break;
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if setup_start.elapsed() > Duration::from_secs(15) {
                        let _ = request
                            .response_tx
                            .send(LiveEvent::Error("Setup timeout".to_string()));
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = request
                        .response_tx
                        .send(LiveEvent::Error(format!("Setup error: {}", e)));
                    break;
                }
            }
        }

        if !setup_complete {
            let _ = socket.close(None);
            continue;
        }

        // Send the actual content
        if let Err(e) = send_live_content(&mut socket, &request.req.content) {
            let _ = request
                .response_tx
                .send(LiveEvent::Error(format!("Send failed: {}", e)));
            let _ = socket.close(None);
            continue;
        }

        // Read response loop
        let mut thinking_sent = false;
        let mut content_started = false;

        loop {
            if !manager.is_generation_valid(request.generation)
                || manager.shutdown.load(Ordering::SeqCst)
            {
                let _ = socket.close(None);
                break;
            }

            match socket.read() {
                Ok(Message::Text(msg)) => {
                    let msg_str = msg.as_str();

                    // Check for errors first
                    if let Some(error) = parse_error(msg_str) {
                        let _ = request.response_tx.send(LiveEvent::Error(error));
                        break;
                    }

                    // Parse response
                    let (text_chunk, is_thought, is_turn_complete) = parse_live_response(msg_str);

                    if let Some(text) = text_chunk {
                        if is_thought {
                            if !thinking_sent && !content_started {
                                let _ = request.response_tx.send(LiveEvent::Thinking);
                                thinking_sent = true;
                            }
                        } else {
                            content_started = true;
                            let _ = request.response_tx.send(LiveEvent::TextChunk(text));
                        }
                    }

                    if is_turn_complete {
                        let _ = request.response_tx.send(LiveEvent::TurnComplete);
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Try to parse as JSON text (ignore raw audio data)
                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                        let (text_chunk, is_thought, is_turn_complete) = parse_live_response(&text);

                        if let Some(chunk) = text_chunk {
                            if is_thought {
                                if !thinking_sent && !content_started {
                                    let _ = request.response_tx.send(LiveEvent::Thinking);
                                    thinking_sent = true;
                                }
                            } else {
                                content_started = true;
                                let _ = request.response_tx.send(LiveEvent::TextChunk(chunk));
                            }
                        }

                        if is_turn_complete {
                            let _ = request.response_tx.send(LiveEvent::TurnComplete);
                            break;
                        }
                    }
                    // Ignore binary audio data (not UTF-8)
                }
                Ok(Message::Close(_)) => {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    let _ = request
                        .response_tx
                        .send(LiveEvent::Error(format!("Read error: {}", e)));
                    break;
                }
            }
        }

        let _ = socket.close(None);
    }
}
