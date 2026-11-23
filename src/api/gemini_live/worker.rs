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

#[cfg(test)]
mod owned_tests;
mod policy;
use super::manager::GeminiLiveManager;
use super::ready_session::{ConnectedLiveSocket, LivePoll, OpenOptions, ReadyLiveSession};
use super::request::RequestControl;
use super::server_frame::LiveServerFrame;
use super::types::LiveEvent;
use super::types::QueuedLiveRequest;
use super::wait::MAX_CANCELLATION_POLL;
use super::websocket::{build_live_setup, send_live_content};
use policy::{ResponsePolicy, ResponseProgress, WaitOutcome};

/// A warm socket idle longer than this may have been silently dropped by a
/// NAT or firewall, so it is closed and replaced by a cold connect.
const WARM_SESSION_MAX_IDLE: Duration = Duration::from_secs(60);

struct WarmLiveSession {
    session: ReadyLiveSession,
    api_key: String,
    model: String,
    instruction: String,
    ready_at: Instant,
}

fn warm_session_matches(
    warm: &Option<WarmLiveSession>,
    api_key: &str,
    model: &str,
    instruction: &str,
    now: Instant,
) -> bool {
    warm.as_ref().is_some_and(|warm| {
        warm_is_fresh(warm.ready_at, now)
            && warm_identity_matches(
                &warm.api_key,
                &warm.model,
                &warm.instruction,
                api_key,
                model,
                instruction,
            )
    })
}

fn warm_is_fresh(ready_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(ready_at) <= WARM_SESSION_MAX_IDLE
}

fn warm_identity_matches(
    warm_api_key: &str,
    warm_model: &str,
    warm_instruction: &str,
    api_key: &str,
    model: &str,
    instruction: &str,
) -> bool {
    warm_api_key == api_key && warm_model == model && warm_instruction == instruction
}

/// Connect + run the setup handshake for `model`, returning a socket ready to
/// receive content. Used both on demand and to PRE-WARM the next socket.
fn connect_and_setup(
    api_key: &str,
    model: &str,
    instruction: Option<&str>,
    show_thinking: bool,
    setup_timeout: Duration,
    cancelled: impl FnMut() -> bool,
    control: Option<&RequestControl>,
) -> anyhow::Result<ReadyLiveSession> {
    let connected = ConnectedLiveSocket::connect(api_key)?;
    if let Some(control) = control {
        control.bind_transport(&connected.shutdown_handle()?)?;
    }
    connected.activate_with(
        build_live_setup(model, instruction, show_thinking),
        OpenOptions {
            setup_timeout,
            setup_read_timeout: MAX_CANCELLATION_POLL,
            active_read_timeout: MAX_CANCELLATION_POLL,
        },
        cancelled,
    )
}

/// Stream one request over a ready socket: send the content, relay text chunks /
/// thinking / completion as `LiveEvent`s. The socket is consumed (closed by the
/// caller after). A stale transport before output permits one cold retry; the
/// stale detail is reported if the retry is stale too.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ServeOutcome {
    Completed,
    Retired,
    Failed,
    Stale(String),
}

/// How one `serve` attempt waits and when it may hand back a stale transport.
#[derive(Clone, Copy)]
struct ServeAttempt {
    policy: ResponsePolicy,
    /// A pre-warmed socket that never produced a server frame is treated as
    /// half-open, so a silent first-output timeout also permits a cold retry.
    reused_warm: bool,
}

impl ServeAttempt {
    fn new(reused_warm: bool) -> Self {
        Self {
            policy: ResponsePolicy::default(),
            reused_warm,
        }
    }
}

fn serve(
    session: &mut ReadyLiveSession,
    request: &QueuedLiveRequest,
    attempt: ServeAttempt,
) -> ServeOutcome {
    if request.is_cancelled_or_expired() {
        request.control.retire();
        return ServeOutcome::Retired;
    }
    if let Err(error) = send_live_content(session, &request.req.content) {
        if request.is_cancelled_or_expired() {
            request.control.retire();
            return ServeOutcome::Retired;
        }
        return ServeOutcome::Stale(format!("send failed: {error}"));
    }
    let interaction_idle = crate::model_config::live_endpoint_profile(&request.req.model)
        .is_some_and(|profile| profile.require_interaction_idle);
    let mut progress = ResponseProgress::new(attempt.policy, interaction_idle, 0);
    let epoch = Instant::now();
    let mut thinking_sent = false;
    let mut frame_seen = false;
    loop {
        if request.is_cancelled_or_expired() {
            request.control.retire();
            return ServeOutcome::Retired;
        }
        let poll_started = Instant::now();
        let poll = session.poll();
        if request.is_cancelled_or_expired() {
            request.control.retire();
            return ServeOutcome::Retired;
        }
        let now = u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut complete = false;
        let mut closed = None;
        let mut idle = false;
        match poll {
            Ok(LivePoll::Frame(frame)) => {
                frame_seen = true;
                let (text, thought) = response_text(&frame);
                if let Some(text) = text {
                    if thought {
                        if !thinking_sent && !progress.content_started() {
                            if !request.publish(LiveEvent::Thinking) {
                                return ServeOutcome::Retired;
                            }
                            thinking_sent = true;
                        }
                    } else if !text.is_empty() {
                        if !request.publish(LiveEvent::TextChunk(text.clone())) {
                            return ServeOutcome::Retired;
                        }
                        progress.observe_text(now, &text);
                    }
                }
                complete = frame.finite_response_complete(interaction_idle);
            }
            Ok(LivePoll::ServerError(error)) => {
                request.publish(LiveEvent::Error(error.message));
                return ServeOutcome::Failed;
            }
            Ok(LivePoll::PeerClosed(frame)) => {
                closed = Some(frame);
            }
            Ok(LivePoll::Unparsed { .. }) => frame_seen = true,
            Ok(LivePoll::Idle) => {
                idle = true;
            }
            Err(error) => {
                if stale_before_output(&progress, now, frame_seen) {
                    return ServeOutcome::Stale(format!("read error: {error}"));
                }
                request.publish(LiveEvent::Error(format!("Read error: {error}")));
                return ServeOutcome::Failed;
            }
        }
        let outcome = progress.outcome(now, complete, closed.is_some());
        match outcome {
            WaitOutcome::Pending => {}
            WaitOutcome::Complete => {
                return if request.publish(LiveEvent::TurnComplete) {
                    ServeOutcome::Completed
                } else {
                    ServeOutcome::Retired
                };
            }
            WaitOutcome::FirstOutputTimeout => {
                if attempt.reused_warm && !frame_seen {
                    return ServeOutcome::Stale(
                        "warm session produced no server frame".to_string(),
                    );
                }
                request.publish(LiveEvent::Error("Response timeout".to_string()));
                return ServeOutcome::Failed;
            }
            WaitOutcome::HardTimeout => {
                request.publish(LiveEvent::Error(
                    "Response did not reach interaction idle before deadline".to_string(),
                ));
                return ServeOutcome::Failed;
            }
            WaitOutcome::Closed => {
                let detail = closed
                    .flatten()
                    .map(|frame| format!("code {}: {}", frame.code, frame.reason))
                    .unwrap_or_else(|| "no close details".to_string());
                if stale_before_output(&progress, now, frame_seen) {
                    return ServeOutcome::Stale(format!("closed ({detail})"));
                }
                request.publish(LiveEvent::Error(format!(
                    "Connection closed before response completion ({detail})"
                )));
                return ServeOutcome::Failed;
            }
        }
        if idle {
            std::thread::sleep(Duration::from_millis(2).saturating_sub(poll_started.elapsed()));
        }
    }
}

/// The cold retry is the last attempt, so a stale transport after it is a
/// final failure whose close/read detail must reach the caller.
fn report_unrecovered_stale(request: &QueuedLiveRequest, outcome: &ServeOutcome) {
    if let ServeOutcome::Stale(detail) = outcome {
        request.publish(LiveEvent::Error(format!(
            "Connection failed before response: {detail}"
        )));
    }
}

/// A transport that failed before any server frame or output may be retried
/// cold; one that already produced output must report the failure.
fn stale_before_output(progress: &ResponseProgress, now_ms: u64, frame_seen: bool) -> bool {
    !progress.content_started() && (now_ms < 500 || !frame_seen)
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
    std::thread::sleep(Duration::from_millis(50));
    let mut warm: Option<WarmLiveSession> = None;
    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }
        let request = {
            let mut queue = manager.work_queue.lock().unwrap();
            while queue.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                queue = manager.work_signal.wait(queue).unwrap();
            }
            if manager.shutdown.load(Ordering::SeqCst) {
                return;
            }
            queue.pop_front()
        };
        let Some(request) = request else { continue };
        if request.is_cancelled_or_expired() {
            request.control.retire();
            continue;
        }
        let api_key = request.req.api_key.clone();
        let model = request.req.model.clone();
        let instruction = request.req.instruction.clone();
        let instruction_option = (!instruction.trim().is_empty()).then_some(instruction.as_str());
        let connect_request = || {
            let timeout = request
                .req
                .remaining()
                .unwrap_or(Duration::from_secs(15))
                .min(Duration::from_secs(15))
                .max(Duration::from_millis(1));
            connect_and_setup(
                &api_key,
                &model,
                instruction_option,
                request.req.show_thinking,
                timeout,
                || request.is_cancelled_or_expired(),
                Some(&request.control),
            )
        };
        let reused_warm =
            warm_session_matches(&warm, &api_key, &model, &instruction, Instant::now());
        let mut session = if reused_warm {
            let ready = warm.take().unwrap().session;
            match ready
                .shutdown_handle()
                .and_then(|socket| request.control.bind_transport(&socket))
            {
                Ok(()) => ready,
                Err(_) => {
                    request.control.retire();
                    continue;
                }
            }
        } else {
            if let Some(mut expired) = warm.take() {
                let _ = expired.session.close();
            }
            match connect_request() {
                Ok(ready) => ready,
                Err(error) => {
                    request.publish(LiveEvent::Error(format!("Connection failed: {error}")));
                    continue;
                }
            }
        };
        let mut outcome = serve(&mut session, &request, ServeAttempt::new(reused_warm));
        if matches!(outcome, ServeOutcome::Stale(_)) && !request.is_cancelled_or_expired() {
            let _ = session.close();
            outcome = match connect_request() {
                Ok(fresh) => {
                    session = fresh;
                    serve(&mut session, &request, ServeAttempt::new(false))
                }
                Err(error) => {
                    request.publish(LiveEvent::Error(format!("Connection failed: {error}")));
                    ServeOutcome::Failed
                }
            };
        }
        report_unrecovered_stale(&request, &outcome);
        let _ = session.close();
        drop(session);
        let show_thinking = request.req.show_thinking;
        let control = request.control.clone();
        let prewarm = outcome == ServeOutcome::Completed
            && control.generation_is_current()
            && !request.req.is_cancelled_or_expired()
            && instruction_option.is_none();
        // Release the caller's channel before the pre-warm so nothing waits
        // behind the next socket's setup.
        drop(request);
        // The next warm socket belongs to the pool, not the completed response.
        warm = if prewarm {
            connect_and_setup(
                &api_key,
                &model,
                None,
                show_thinking,
                Duration::from_secs(15),
                || !control.generation_is_current(),
                None,
            )
            .ok()
            .map(|session| WarmLiveSession {
                session,
                api_key,
                model,
                instruction,
                ready_at: Instant::now(),
            })
        } else {
            None
        };
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

    #[test]
    fn warm_session_expires_after_idle_ttl() {
        let ready_at = Instant::now();
        assert!(warm_is_fresh(ready_at, ready_at + WARM_SESSION_MAX_IDLE));
        assert!(!warm_is_fresh(
            ready_at,
            ready_at + WARM_SESSION_MAX_IDLE + Duration::from_millis(1)
        ));
        assert!(!warm_session_matches(&None, "k", "m", "i", ready_at));
    }

    #[test]
    fn warm_session_identity_includes_the_exact_credential() {
        assert!(warm_identity_matches(
            "key-a",
            "model",
            "instruction",
            "key-a",
            "model",
            "instruction"
        ));
        assert!(!warm_identity_matches(
            "key-a",
            "model",
            "instruction",
            "key-b",
            "model",
            "instruction"
        ));
    }
}
