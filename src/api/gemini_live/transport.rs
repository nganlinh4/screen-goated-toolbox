//! Shared transport primitives for Gemini Live WebSocket consumers.

use std::io;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::Result;

mod connect;
mod handshake;
pub(crate) use handshake::{tls_handshake, websocket_handshake};

/// Hard bound on DNS, TCP, TLS and WebSocket upgrade for one Live connection.
const CONNECT_BUDGET: Duration = Duration::from_secs(15);

/// Bounded shared DNS and TCP connection path for WebSocket providers.
pub(crate) fn connect_tcp_until(
    host: &str,
    port: u16,
    deadline: std::time::Instant,
    cancelled: &impl Fn() -> bool,
) -> io::Result<TcpStream> {
    connect::connect_until(host, port, deadline, cancelled)
}

/// Canonical Gemini Live BidiGenerateContent endpoint.
pub const GEMINI_LIVE_WS_BASE_URL: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

pub type LiveSocket = tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>;

/// Create a TLS WebSocket connection to the canonical Gemini Live endpoint.
pub fn connect_websocket(api_key: &str) -> Result<LiveSocket> {
    connect_websocket_to(GEMINI_LIVE_WS_BASE_URL, api_key)
}

/// Create a TLS WebSocket connection to the canonical endpoint that gives up
/// when `cancelled` turns true instead of finishing a slow connection setup.
#[cfg(not(feature = "recorder-worker"))]
pub fn connect_websocket_cancellable(
    api_key: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<LiveSocket> {
    connect_websocket_until(
        GEMINI_LIVE_WS_BASE_URL,
        api_key,
        Instant::now() + CONNECT_BUDGET,
        cancelled,
    )
}

/// Create a TLS WebSocket connection to an override endpoint.
pub fn connect_websocket_to(base_url: &str, api_key: &str) -> Result<LiveSocket> {
    connect_websocket_until(base_url, api_key, Instant::now() + CONNECT_BUDGET, &|| {
        false
    })
}

/// Resolve, connect, and complete TLS and the WebSocket upgrade before one
/// absolute deadline. Every stage polls `cancelled`; a silent peer cannot hold
/// the caller beyond `deadline` at any stage.
pub(crate) fn connect_websocket_until(
    base_url: &str,
    api_key: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<LiveSocket> {
    let url = websocket_url(base_url, api_key)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
    let host = socket_hostname(host);
    let port = url.port_or_known_default().unwrap_or(443);

    let tcp_deadline = deadline.min(Instant::now() + connect::TOTAL_BUDGET);
    let tcp_stream = connect::connect_until(host, port, tcp_deadline, cancelled)?;
    tcp_stream.set_nodelay(true)?;
    tcp_stream.set_nonblocking(true)?;

    let connector = native_tls::TlsConnector::new()?;
    let tls_stream = tls_handshake(&connector, host, tcp_stream, deadline, cancelled)
        .map_err(handshake::HandshakeFailure::into_anyhow)?;
    let mut socket = websocket_handshake(url.as_str(), tls_stream, None, deadline, cancelled)
        .map_err(handshake::HandshakeFailure::into_anyhow)?;

    let stream = socket.get_mut().get_mut();
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    Ok(socket)
}

fn websocket_url(base_url: &str, api_key: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(base_url)?;
    url.query_pairs_mut().append_pair("key", api_key);
    Ok(url)
}

fn socket_hostname(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host)
}

/// Use a short read timeout for normal Live session loops.
pub fn set_socket_nonblocking(socket: &mut LiveSocket) -> Result<()> {
    socket
        .get_mut()
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(50)))?;
    Ok(())
}

/// Use a bounded setup timeout so callers can observe cancellation/model changes.
pub fn set_socket_short_timeout(socket: &mut LiveSocket) -> Result<()> {
    socket
        .get_mut()
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(())
}

pub fn is_transient_socket_read_error(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(err) if is_transient_read_io_error(err))
}

#[cfg(feature = "recorder-worker")]
pub fn is_recoverable_socket_error(error: &tungstenite::Error) -> bool {
    if is_transient_socket_read_error(error) {
        return true;
    }
    match error {
        tungstenite::Error::Io(err) => is_recoverable_io_error(err),
        _ => is_recoverable_socket_error_text(&error.to_string()),
    }
}

pub fn is_transient_read_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    ) || error.raw_os_error() == Some(997)
        || error
            .to_string()
            .contains("Overlapped I/O operation is in progress")
}

#[cfg(feature = "recorder-worker")]
pub fn is_recoverable_io_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(-2146893008)
        || is_recoverable_socket_error_text(&error.to_string())
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_transient_anyhow_io_error(error: &anyhow::Error) -> bool {
    let detail = format!("{error:?}");
    detail.contains("os error 997") || detail.contains("Overlapped I/O operation is in progress")
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_recoverable_anyhow_socket_error(error: &anyhow::Error) -> bool {
    if is_transient_anyhow_io_error(error) {
        return true;
    }
    is_recoverable_socket_error_text(&format!("{error:?}"))
}

fn is_recoverable_socket_error_text(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("reset")
        || lowered.contains("closed")
        || lowered.contains("broken")
        || lowered.contains("could not be decrypted")
        || lowered.contains("os error -2146893008")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_query_encodes_api_key() {
        let url = websocket_url(GEMINI_LIVE_WS_BASE_URL, "key+/=?").unwrap();
        assert_eq!(
            url.query_pairs().find(|(name, _)| name == "key").unwrap().1,
            "key+/=?"
        );
    }

    #[test]
    fn silent_endpoint_cannot_hold_connection_setup_past_the_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("wss://{}/live", listener.local_addr().unwrap());
        let accept = std::thread::spawn(move || listener.accept().map(|(peer, _)| peer));
        let started = Instant::now();
        let error =
            connect_websocket_until(&base, "key", started + Duration::from_millis(300), &|| {
                false
            })
            .map(|_| ())
            .unwrap_err();
        assert_eq!(error.to_string(), "connection setup timed out");
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(accept.join());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("wss://{}/live", listener.local_addr().unwrap());
        let accept = std::thread::spawn(move || listener.accept().map(|(peer, _)| peer));
        let started = Instant::now();
        let cancelled = || started.elapsed() >= Duration::from_millis(100);
        let error = connect_websocket_until(&base, "key", started + CONNECT_BUDGET, &cancelled)
            .map(|_| ())
            .unwrap_err();
        assert_eq!(error.to_string(), "connection setup cancelled");
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(accept.join());
    }

    #[test]
    fn ipv6_url_host_is_available_without_brackets_for_resolution() {
        let url = websocket_url("wss://[::1]:443/live", "key").unwrap();
        let host = url.host_str().unwrap();
        let normalized = socket_hostname(host);
        assert_eq!(
            normalized.parse::<std::net::IpAddr>().unwrap(),
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        );
    }
}
