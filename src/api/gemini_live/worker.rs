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

use super::manager::GeminiLiveManager;
use super::ready_session::{ConnectedLiveSocket, LivePoll, OpenOptions, ReadyLiveSession};
use super::server_frame::LiveServerFrame;
use super::types::LiveEvent;
use super::websocket::{build_live_setup, send_live_content};
use crate::APP;

/// Connect + run the setup handshake for `model`, returning a socket ready to
/// receive content. Used both on demand and to PRE-WARM the next socket.
fn connect_and_setup(
    api_key: &str,
    model: &str,
    instruction: Option<&str>,
    show_thinking: bool,
) -> anyhow::Result<ReadyLiveSession> {
    ConnectedLiveSocket::connect(api_key)?.activate_with(
        build_live_setup(model, instruction, show_thinking),
        OpenOptions {
            active_read_timeout: Duration::from_millis(250),
            ..OpenOptions::default()
        },
        || false,
    )
}

/// Stream one request over a ready socket: send the content, relay text chunks /
/// thinking / completion as `LiveEvent`s. The socket is consumed (closed by the
/// caller after). Returns Err only if the SOCKET itself failed before any content
/// (so a stale warm socket can be retried cold).
fn serve(
    session: &mut ReadyLiveSession,
    request: &super::types::QueuedLiveRequest,
    manager: &GeminiLiveManager,
) -> Result<(), ()> {
    let send_result = send_live_content(session, &request.req.content);
    if send_result.is_err() {
        // No content was produced — signal the caller it can retry on a fresh socket.
        return Err(());
    }

    let mut thinking_sent = false;
    let mut content_started = false;
    let response_start = Instant::now();
    let response_timeout = Duration::from_secs(20);
    let idle_finalize_after = Duration::from_millis(1200);
    let mut last_content_at: Option<Instant> = None;

    loop {
        if !manager.is_generation_valid(request.generation)
            || manager.shutdown.load(Ordering::SeqCst)
        {
            break;
        }

        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                let (text_chunk, is_thought) = response_text(&frame);
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
                if frame.response_complete() {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                    break;
                }
            }
            Ok(LivePoll::ServerError(error)) => {
                let _ = request.response_tx.send(LiveEvent::Error(error.message));
                break;
            }
            Ok(LivePoll::PeerClosed(frame)) => {
                if content_started {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                } else {
                    let detail = frame
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
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::Idle) => {
                if let Some(last) = last_content_at
                    && last.elapsed() >= idle_finalize_after
                {
                    let _ = request.response_tx.send(LiveEvent::TurnComplete);
                    break;
                }
                if !content_started && response_start.elapsed() >= response_timeout {
                    let _ = request
                        .response_tx
                        .send(LiveEvent::Error("Response timeout".to_string()));
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                if !content_started && response_start.elapsed() < Duration::from_millis(500) {
                    return Err(()); // stale warm socket — let caller retry cold
                }
                let _ = request
                    .response_tx
                    .send(LiveEvent::Error(format!("Read error: {e}")));
                break;
            }
        }
    }
    Ok(())
}

fn response_text(frame: &LiveServerFrame) -> (Option<String>, bool) {
    let mut visible = Vec::new();
    let mut thoughts = Vec::new();

    if let Some(transcript) = frame
        .output_transcript
        .as_ref()
        .filter(|text| !text.chars().all(char::is_whitespace))
    {
        visible.push(transcript.as_str());
    }
    for part in &frame.text_parts {
        if part.thought {
            thoughts.push(part.text.as_str());
        } else {
            visible.push(part.text.as_str());
        }
    }

    if !visible.is_empty() {
        return (Some(visible.concat()), false);
    }
    if !thoughts.is_empty() {
        return (Some(thoughts.concat()), true);
    }
    (None, false)
}

/// Run a worker thread for the Gemini Live connection pool.
pub fn run_live_worker(manager: Arc<GeminiLiveManager>) {
    std::thread::sleep(Duration::from_millis(50)); // stagger startup
    // A single pre-warmed socket for the last (model, instruction) we served.
    let mut warm: Option<(ReadyLiveSession, String, String)> = None;

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
            let _ = request
                .response_tx
                .send(LiveEvent::Error("Request cancelled".to_string()));
            continue;
        }

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
            let lang = APP
                .lock()
                .ok()
                .map(|a| a.config.ui_language.clone())
                .unwrap_or_else(|| "en".to_string());
            crate::overlay::utils::show_api_key_error_notification("NO_API_KEY:gemini", &lang);
            let _ = request
                .response_tx
                .send(LiveEvent::Error("NO_API_KEY:gemini".to_string()));
            continue;
        }

        let model = request.req.model.clone();
        let instruction = request.req.instruction.clone();
        let instr_opt = (!instruction.trim().is_empty()).then_some(instruction.as_str());

        // Reuse the pre-warmed socket if it matches this (model, instruction);
        // otherwise connect cold. On a stale warm socket, fall back to cold once.
        let warm_match = matches!(&warm, Some((_, m, i)) if *m == model && *i == instruction);
        let mut session = if warm_match {
            warm.take().map(|(s, _, _)| s).unwrap()
        } else {
            warm = None; // wrong model/instruction — drop the warm one
            match connect_and_setup(&api_key, &model, instr_opt, request.req.show_thinking) {
                Ok(s) => s,
                Err(e) => {
                    let _ = request
                        .response_tx
                        .send(LiveEvent::Error(format!("Connection failed: {e}")));
                    continue;
                }
            }
        };

        if serve(&mut session, &request, &manager).is_err() {
            // Warm socket was stale: reconnect cold and serve once more.
            let _ = session.close();
            match connect_and_setup(&api_key, &model, instr_opt, request.req.show_thinking) {
                Ok(mut fresh) => {
                    let _ = serve(&mut fresh, &request, &manager);
                    let _ = fresh.close();
                }
                Err(e) => {
                    let _ = request
                        .response_tx
                        .send(LiveEvent::Error(format!("Connection failed: {e}")));
                }
            }
        } else {
            let _ = session.close();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::gemini_live::server_frame::LiveTextPart;

    #[test]
    fn visible_response_text_wins_over_thoughts() {
        let frame = LiveServerFrame {
            output_transcript: Some("spoken ".to_string()),
            text_parts: vec![
                LiveTextPart {
                    text: "hidden".to_string(),
                    thought: true,
                },
                LiveTextPart {
                    text: "answer".to_string(),
                    thought: false,
                },
            ],
            ..LiveServerFrame::default()
        };

        assert_eq!(
            response_text(&frame),
            (Some("spoken answer".to_string()), false)
        );
    }

    #[test]
    fn thought_only_response_retains_thinking_signal() {
        let frame = LiveServerFrame {
            text_parts: vec![LiveTextPart {
                text: "reasoning".to_string(),
                thought: true,
            }],
            ..LiveServerFrame::default()
        };

        assert_eq!(response_text(&frame), (Some("reasoning".to_string()), true));
    }
}
