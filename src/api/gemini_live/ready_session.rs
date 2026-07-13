//! Setup-gated Gemini Live WebSocket sessions.
//!
//! A [`ConnectedLiveSocket`] owns a transport connection that is not yet safe
//! for normal Live traffic. It can become a [`ReadyLiveSession`] only after the
//! server structurally acknowledges the setup payload. Feature-specific loops
//! remain responsible for reconnect, replay, completion, and shutdown policy.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use tungstenite::Message;

use super::server_frame::{LiveServerFrame, parse_server_frame};
use super::transport::{
    LiveSocket, connect_websocket, connect_websocket_to, is_transient_socket_read_error,
};

/// Timeouts used while promoting a connected transport into a ready session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenOptions {
    /// Maximum time to wait for the server's top-level `setupComplete` field.
    pub setup_timeout: Duration,
    /// Read timeout while setup is pending, which bounds cancellation latency.
    pub setup_read_timeout: Duration,
    /// Read timeout installed only after setup has been acknowledged.
    pub active_read_timeout: Duration,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            setup_timeout: Duration::from_secs(15),
            setup_read_timeout: Duration::from_millis(200),
            active_read_timeout: Duration::from_millis(50),
        }
    }
}

impl OpenOptions {
    fn validate(self) -> Result<Self> {
        ensure!(
            !self.setup_timeout.is_zero(),
            "setup timeout must be non-zero"
        );
        ensure!(
            !self.setup_read_timeout.is_zero(),
            "setup read timeout must be non-zero"
        );
        ensure!(
            !self.active_read_timeout.is_zero(),
            "active read timeout must be non-zero"
        );
        Ok(self)
    }
}

/// Owned details from a peer WebSocket close frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCloseInfo {
    pub code: u16,
    pub reason: String,
}

/// A structurally classified top-level Gemini Live server error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveServerError {
    pub message: String,
    pub retryable: bool,
}

impl fmt::Display for LiveServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LiveServerError {}

/// A server error received before setup activation completed.
#[derive(Debug)]
pub struct LiveSetupServerError {
    pub server: LiveServerError,
}

impl fmt::Display for LiveSetupServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Gemini Live setup error: {}", self.server)
    }
}

impl Error for LiveSetupServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.server)
    }
}

/// One structurally classified read from an active Gemini Live session.
#[derive(Clone, Debug, PartialEq)]
pub enum LivePoll {
    /// A decoded Gemini Live protocol frame.
    Frame(Box<LiveServerFrame>),
    /// No protocol frame arrived before the active read timeout.
    Idle,
    /// A text or binary payload was not valid Gemini Live JSON.
    Unparsed {
        payload: String,
        wire_format: LiveWireFormat,
    },
    /// The peer sent a WebSocket close frame (or closed without one).
    PeerClosed(Option<LiveCloseInfo>),
    /// The server sent a top-level Gemini Live error payload.
    ServerError(LiveServerError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveWireFormat {
    Text,
    Binary,
}

/// A TLS WebSocket transport that has not completed Gemini Live setup.
pub struct ConnectedLiveSocket {
    socket: LiveSocket,
}

impl ConnectedLiveSocket {
    /// Connect to the canonical Gemini Live endpoint.
    pub fn connect(api_key: &str) -> Result<Self> {
        Ok(Self::from_socket(connect_websocket(api_key)?))
    }

    /// Connect to an alternate Gemini Live endpoint, primarily for protocol
    /// probes and endpoint-version testing.
    pub fn connect_to(base_url: &str, api_key: &str) -> Result<Self> {
        Ok(Self::from_socket(connect_websocket_to(base_url, api_key)?))
    }

    /// Wrap an already-connected transport, such as a transport-only warm pool
    /// entry. No setup readiness is inferred from the raw socket.
    pub fn from_socket(socket: LiveSocket) -> Self {
        Self { socket }
    }

    /// Send setup and wait for its structural acknowledgment using default
    /// timeouts and no cancellation source.
    pub fn activate(self, setup: Value) -> Result<ReadyLiveSession> {
        self.activate_with(setup, OpenOptions::default(), || false)
    }

    /// Promote this transport into an active session.
    ///
    /// The setup polling timeout is installed before the payload is written so
    /// deadline and cancellation checks cannot be hidden behind the transport's
    /// longer connection-time read timeout. A top-level error wins even if the
    /// same frame also contains `setupComplete`.
    pub fn activate_with(
        self,
        setup: Value,
        options: OpenOptions,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReadyLiveSession> {
        let options = options.validate()?;
        let mut socket = self.socket;
        let mut pending_polls = VecDeque::new();
        let deadline = Instant::now()
            .checked_add(options.setup_timeout)
            .context("setup deadline overflow")?;

        // This must happen before setup is sent. Otherwise the first read can
        // block for the transport's 30-second connection timeout.
        set_read_timeout(
            &mut socket,
            options.setup_read_timeout.min(options.setup_timeout),
        )
        .context("set Gemini Live setup read timeout")?;

        ensure!(!cancelled(), "Gemini Live setup cancelled");
        socket
            .write(Message::Text(setup.to_string().into()))
            .context("send Gemini Live setup")?;
        socket.flush().context("flush Gemini Live setup")?;

        loop {
            ensure!(!cancelled(), "Gemini Live setup cancelled");
            let now = Instant::now();
            ensure!(now < deadline, "Gemini Live setup timed out");

            // Tighten the final poll so the configured deadline, rather than a
            // longer read timeout, remains the upper bound.
            set_read_timeout(
                &mut socket,
                options.setup_read_timeout.min(deadline.duration_since(now)),
            )
            .context("update Gemini Live setup read timeout")?;

            let read_result = socket.read();
            ensure!(!cancelled(), "Gemini Live setup cancelled");
            ensure!(Instant::now() < deadline, "Gemini Live setup timed out");

            let signal = match read_result {
                Ok(Message::Text(text)) => {
                    classify_setup_message(text.as_str(), LiveWireFormat::Text)
                }
                Ok(Message::Binary(bytes)) => {
                    let text = String::from_utf8_lossy(&bytes);
                    classify_setup_message(&text, LiveWireFormat::Binary)
                }
                Ok(Message::Close(frame)) => {
                    let detail = close_detail(frame.as_ref());
                    anyhow::bail!("Gemini Live peer closed during setup{detail}")
                }
                Ok(_) => continue,
                Err(error) if is_transient_socket_read_error(&error) => {
                    // Some Windows WouldBlock/overlapped-I/O errors can return
                    // immediately even with a read timeout installed.
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => return Err(error).context("read Gemini Live setup response"),
            };

            match signal {
                SetupSignal::Pending(poll) => pending_polls.extend(poll),
                SetupSignal::Complete(poll) => {
                    pending_polls.extend(poll);
                    break;
                }
                SetupSignal::ServerError(error) => {
                    return Err(LiveSetupServerError { server: error }.into());
                }
            }
        }

        set_read_timeout(&mut socket, options.active_read_timeout)
            .context("set Gemini Live active read timeout")?;
        Ok(ReadyLiveSession {
            socket,
            closed: false,
            pending_polls,
        })
    }
}

/// A Gemini Live transport whose setup has been acknowledged by the server.
pub struct ReadyLiveSession {
    socket: LiveSocket,
    closed: bool,
    pending_polls: VecDeque<LivePoll>,
}

impl ReadyLiveSession {
    /// Send and flush one JSON protocol payload.
    pub fn send_json(&mut self, payload: &Value) -> Result<()> {
        ensure!(!self.closed, "Gemini Live session is closed");
        self.socket
            .write(Message::Text(payload.to_string().into()))
            .context("send Gemini Live payload")?;
        self.socket.flush().context("flush Gemini Live payload")?;
        Ok(())
    }

    /// Send one PCM16 realtime-input frame after setup acknowledgement.
    pub fn send_audio_pcm(&mut self, samples: &[i16], sample_rate: u32) -> Result<()> {
        self.send_json(&super::client_message::realtime_audio_pcm(
            samples,
            sample_rate,
        ))
    }

    /// Send already-encoded little-endian PCM bytes after setup acknowledgement.
    pub fn send_audio_bytes(&mut self, bytes: &[u8], sample_rate: u32) -> Result<()> {
        self.send_json(&super::client_message::realtime_audio_bytes(
            bytes,
            sample_rate,
        ))
    }

    /// End the current realtime audio input stream.
    pub fn end_audio_stream(&mut self) -> Result<()> {
        self.send_json(&super::client_message::audio_stream_end())
    }

    /// Read and structurally classify one server message.
    pub fn poll(&mut self) -> Result<LivePoll> {
        if let Some(poll) = self.pending_polls.pop_front() {
            return Ok(poll);
        }
        if self.closed {
            return Ok(LivePoll::PeerClosed(None));
        }

        match self.socket.read() {
            Ok(Message::Text(text)) => {
                Ok(classify_active_message(text.as_str(), LiveWireFormat::Text))
            }
            Ok(Message::Binary(bytes)) => {
                let text = String::from_utf8_lossy(&bytes);
                Ok(classify_active_message(&text, LiveWireFormat::Binary))
            }
            Ok(Message::Close(frame)) => {
                self.closed = true;
                Ok(LivePoll::PeerClosed(frame.as_ref().map(close_info)))
            }
            Ok(_) => Ok(LivePoll::Idle),
            Err(error) if is_transient_socket_read_error(&error) => Ok(LivePoll::Idle),
            Err(error) => Err(error).context("read Gemini Live session"),
        }
    }

    /// Best-effort protocol close. Repeated closes are harmless.
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.socket.close(None).context("close Gemini Live session")
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SetupSignal {
    Pending(Option<LivePoll>),
    Complete(Option<LivePoll>),
    ServerError(LiveServerError),
}

fn classify_setup_message(message: &str, wire_format: LiveWireFormat) -> SetupSignal {
    let Ok(frame) = parse_server_frame(message) else {
        return SetupSignal::Pending(Some(LivePoll::Unparsed {
            payload: message.to_string(),
            wire_format,
        }));
    };
    if let Some(error) = frame.error {
        return SetupSignal::ServerError(LiveServerError {
            message: error,
            retryable: frame.error_retryable,
        });
    }
    let setup_complete = frame.setup_complete;
    let post_setup_poll = (!setup_complete || frame.has_post_setup_observation())
        .then_some(LivePoll::Frame(Box::new(frame)));
    if setup_complete {
        return SetupSignal::Complete(post_setup_poll);
    }
    SetupSignal::Pending(post_setup_poll)
}

fn classify_active_message(message: &str, wire_format: LiveWireFormat) -> LivePoll {
    let Ok(frame) = parse_server_frame(message) else {
        return LivePoll::Unparsed {
            payload: message.to_string(),
            wire_format,
        };
    };
    if let Some(error) = frame.error.clone() {
        return LivePoll::ServerError(LiveServerError {
            message: error,
            retryable: frame.error_retryable,
        });
    }
    LivePoll::Frame(Box::new(frame))
}

fn set_read_timeout(socket: &mut LiveSocket, timeout: Duration) -> Result<()> {
    ensure!(!timeout.is_zero(), "socket read timeout must be non-zero");
    socket.get_mut().get_mut().set_read_timeout(Some(timeout))?;
    Ok(())
}

fn close_info(frame: &tungstenite::protocol::CloseFrame) -> LiveCloseInfo {
    LiveCloseInfo {
        code: frame.code.into(),
        reason: frame.reason.to_string(),
    }
}

fn close_detail(frame: Option<&tungstenite::protocol::CloseFrame>) -> String {
    frame.map_or_else(String::new, |frame| {
        let info = close_info(frame);
        if info.reason.is_empty() {
            format!(" (code {})", info.code)
        } else {
            format!(" (code {}: {})", info.code, info.reason)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_error_wins_over_setup_completion() {
        let signal = classify_setup_message(
            r#"{"setupComplete":{},"error":{"message":"rejected"}}"#,
            LiveWireFormat::Text,
        );
        assert_eq!(
            signal,
            SetupSignal::ServerError(LiveServerError {
                message: "rejected".to_string(),
                retryable: false,
            })
        );
    }

    #[test]
    fn setup_completion_requires_structural_protocol_field() {
        assert_eq!(
            classify_setup_message(r#"{"setupComplete":{}}"#, LiveWireFormat::Text,),
            SetupSignal::Complete(None)
        );
        assert!(matches!(
            classify_setup_message(r#"{"errorText":"setupComplete"}"#, LiveWireFormat::Text,),
            SetupSignal::Pending(Some(LivePoll::Frame(_)))
        ));
    }

    #[test]
    fn setup_frame_content_is_preserved_for_the_ready_session() {
        let signal = classify_setup_message(
            r#"{"setupComplete":{},"serverContent":{"modelTurn":{"parts":[{"text":"ready"}]}}}"#,
            LiveWireFormat::Text,
        );
        let SetupSignal::Complete(Some(LivePoll::Frame(frame))) = signal else {
            panic!("expected combined setup frame to be retained");
        };
        assert_eq!(frame.text_parts[0].text, "ready");
    }

    #[test]
    fn active_error_is_not_reported_as_a_healthy_frame() {
        let poll = classify_active_message(
            r#"{"serverContent":{"turnComplete":true},"error":{"message":"failed"}}"#,
            LiveWireFormat::Text,
        );
        assert_eq!(
            poll,
            LivePoll::ServerError(LiveServerError {
                message: "failed".to_string(),
                retryable: false,
            })
        );
    }

    #[test]
    fn active_transient_server_error_preserves_retryability() {
        let poll = classify_active_message(
            r#"{"error":{"code":503,"status":"UNAVAILABLE","message":"retry"}}"#,
            LiveWireFormat::Text,
        );
        assert_eq!(
            poll,
            LivePoll::ServerError(LiveServerError {
                message: "retry".to_string(),
                retryable: true,
            })
        );
    }

    #[test]
    fn active_frame_preserves_distinct_completion_signals() {
        let poll = classify_active_message(
            r#"{"serverContent":{"generationComplete":true,"turnComplete":false}}"#,
            LiveWireFormat::Text,
        );
        let LivePoll::Frame(frame) = poll else {
            panic!("expected structural frame");
        };
        assert!(frame.generation_complete);
        assert!(!frame.turn_complete);
    }

    #[test]
    fn open_options_reject_zero_timeouts() {
        let invalid = OpenOptions {
            setup_timeout: Duration::ZERO,
            ..OpenOptions::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn malformed_payload_is_nonterminal_and_preserves_wire_format() {
        assert_eq!(
            classify_active_message("not-json", LiveWireFormat::Binary),
            LivePoll::Unparsed {
                payload: "not-json".to_string(),
                wire_format: LiveWireFormat::Binary,
            }
        );
    }
}
