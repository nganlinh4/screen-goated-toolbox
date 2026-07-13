//! Shared transport primitives for Gemini Live WebSocket consumers.

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::Result;

/// Canonical Gemini Live BidiGenerateContent endpoint.
pub const GEMINI_LIVE_WS_BASE_URL: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

pub type LiveSocket = tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>;

/// Create a TLS WebSocket connection to the canonical Gemini Live endpoint.
pub fn connect_websocket(api_key: &str) -> Result<LiveSocket> {
    connect_websocket_to(GEMINI_LIVE_WS_BASE_URL, api_key)
}

/// Create a TLS WebSocket connection to an override endpoint.
pub fn connect_websocket_to(base_url: &str, api_key: &str) -> Result<LiveSocket> {
    let url = websocket_url(base_url, api_key)?;
    let ws_url = url.as_str();
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
    let port = url.port_or_known_default().unwrap_or(443);

    let addr = format!("{host}:{port}")
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve hostname: {host}"))?;

    let tcp_stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
    tcp_stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_nodelay(true)?;

    let connector = native_tls::TlsConnector::new()?;
    let tls_stream = connector.connect(host, tcp_stream)?;
    let (socket, _response) = tungstenite::client::client(ws_url, tls_stream)?;
    Ok(socket)
}

fn websocket_url(base_url: &str, api_key: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(base_url)?;
    url.query_pairs_mut().append_pair("key", api_key);
    Ok(url)
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

pub fn is_recoverable_io_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(-2146893008)
        || is_recoverable_socket_error_text(&error.to_string())
}

pub fn is_transient_anyhow_io_error(error: &anyhow::Error) -> bool {
    let detail = format!("{error:?}");
    detail.contains("os error 997") || detail.contains("Overlapped I/O operation is in progress")
}

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
}
