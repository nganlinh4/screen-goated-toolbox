//! TLS and WebSocket client handshakes bounded by one absolute deadline.
//!
//! Both handshakes run on a nonblocking socket so a silent or slow peer cannot
//! hold a caller past its deadline, and cancellation is observed between I/O
//! readiness polls instead of after a blocking read timeout.

use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const IO_POLL: Duration = Duration::from_millis(10);

/// Why a bounded connection setup did not produce a socket.
#[derive(Debug)]
pub(crate) enum HandshakeFailure {
    Cancelled,
    TimedOut,
    Tls(native_tls::Error),
    WebSocket(tungstenite::Error),
}

impl fmt::Display for HandshakeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("connection setup cancelled"),
            Self::TimedOut => formatter.write_str("connection setup timed out"),
            Self::Tls(error) => write!(formatter, "TLS handshake failed: {error}"),
            Self::WebSocket(error) => write!(formatter, "WebSocket connection failed: {error}"),
        }
    }
}

impl std::error::Error for HandshakeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tls(error) => Some(error),
            Self::WebSocket(error) => Some(error),
            Self::Cancelled | Self::TimedOut => None,
        }
    }
}

impl HandshakeFailure {
    /// Keeps a WebSocket failure downcastable to `tungstenite::Error`, so an
    /// HTTP upgrade rejection is still classified structurally by callers.
    pub(crate) fn into_anyhow(self) -> anyhow::Error {
        match self {
            Self::WebSocket(error) => anyhow::Error::new(error),
            other => anyhow::Error::msg(other.to_string()),
        }
    }
}

fn check(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), HandshakeFailure> {
    if cancelled() {
        return Err(HandshakeFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(HandshakeFailure::TimedOut);
    }
    Ok(())
}

fn wait_for_io(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), HandshakeFailure> {
    check(deadline, cancelled)?;
    std::thread::sleep(IO_POLL.min(deadline.saturating_duration_since(Instant::now())));
    check(deadline, cancelled)
}

/// Complete a TLS client handshake on a nonblocking stream before `deadline`.
pub(crate) fn tls_handshake(
    connector: &native_tls::TlsConnector,
    host: &str,
    stream: TcpStream,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<native_tls::TlsStream<TcpStream>, HandshakeFailure> {
    check(deadline, cancelled)?;
    let mut handshake = connector.connect(host, stream);
    loop {
        match handshake {
            Ok(stream) => {
                check(deadline, cancelled)?;
                return Ok(stream);
            }
            Err(native_tls::HandshakeError::WouldBlock(pending)) => {
                wait_for_io(deadline, cancelled)?;
                handshake = pending.handshake();
            }
            Err(native_tls::HandshakeError::Failure(error)) => {
                return Err(HandshakeFailure::Tls(error));
            }
        }
    }
}

/// Complete a WebSocket client upgrade on a nonblocking stream before `deadline`.
pub(crate) fn websocket_handshake<S: Read + Write>(
    request: impl tungstenite::client::IntoClientRequest,
    stream: S,
    config: Option<tungstenite::protocol::WebSocketConfig>,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<tungstenite::WebSocket<S>, HandshakeFailure> {
    check(deadline, cancelled)?;
    let mut handshake = tungstenite::client::client_with_config(request, stream, config);
    loop {
        match handshake {
            Ok((socket, _response)) => {
                check(deadline, cancelled)?;
                return Ok(socket);
            }
            Err(tungstenite::HandshakeError::Interrupted(pending)) => {
                wait_for_io(deadline, cancelled)?;
                handshake = pending.handshake();
            }
            Err(tungstenite::HandshakeError::Failure(error)) => {
                return Err(HandshakeFailure::WebSocket(error));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use tungstenite::client::IntoClientRequest;

    fn stalled_stream() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        client.set_nonblocking(true).unwrap();
        (client, peer)
    }

    fn run(tls: bool, deadline: Instant, cancelled: &impl Fn() -> bool) -> HandshakeFailure {
        let (client, _peer) = stalled_stream();
        if tls {
            tls_handshake(
                &native_tls::TlsConnector::new().unwrap(),
                "localhost",
                client,
                deadline,
                cancelled,
            )
            .map(|_| ())
            .unwrap_err()
        } else {
            websocket_handshake(
                "ws://localhost/".into_client_request().unwrap(),
                client,
                None,
                deadline,
                cancelled,
            )
            .map(|_| ())
            .unwrap_err()
        }
    }

    #[test]
    fn silent_peer_cannot_hold_tls_or_upgrade_past_the_deadline() {
        for tls in [true, false] {
            let started = Instant::now();
            let failure = run(tls, started + Duration::from_millis(100), &|| false);
            assert!(matches!(failure, HandshakeFailure::TimedOut), "{failure}");
            assert!(started.elapsed() < Duration::from_secs(1));
        }
    }

    #[test]
    fn cancellation_interrupts_a_pending_handshake() {
        for tls in [true, false] {
            let started = Instant::now();
            let cancelled = || started.elapsed() >= Duration::from_millis(60);
            let failure = run(tls, started + Duration::from_secs(30), &cancelled);
            assert!(matches!(failure, HandshakeFailure::Cancelled), "{failure}");
            assert!(started.elapsed() < Duration::from_secs(1));
        }
    }

    #[test]
    fn upgrade_rejection_stays_a_structured_websocket_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "request ended before headers");
                request.extend_from_slice(&buffer[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let client = TcpStream::connect(address).unwrap();
        client.set_nonblocking(true).unwrap();
        let failure = websocket_handshake(
            format!("ws://{address}/").into_client_request().unwrap(),
            client,
            None,
            Instant::now() + Duration::from_secs(5),
            &|| false,
        )
        .map(|_| ())
        .unwrap_err();
        server.join().unwrap();
        let error = failure.into_anyhow();
        let Some(tungstenite::Error::Http(response)) = error.downcast_ref::<tungstenite::Error>()
        else {
            panic!("upgrade rejection lost its structure: {error:?}");
        };
        assert_eq!(response.status().as_u16(), 403);
    }
}
