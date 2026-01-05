//! WebSocket connection and communication for Gemini Live API

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use std::net::TcpStream;
use std::time::Duration;

use super::REALTIME_MODEL;

/// Create TLS WebSocket connection to Gemini Live API
pub fn connect_websocket(
    api_key: &str,
) -> Result<tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>> {
    let ws_url = format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={}",
        api_key
    );

    let url = url::Url::parse(&ws_url)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
    let port = 443;

    // Resolve hostname to IP address first
    use std::net::ToSocketAddrs;
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve hostname: {}", host))?;

    // Connect TCP with a long timeout for initial handshake
    let tcp_stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
    // Use blocking mode with long timeout during setup
    tcp_stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_nodelay(true)?;

    // Wrap with TLS
    let connector = native_tls::TlsConnector::new()?;
    let tls_stream = connector.connect(host, tcp_stream)?;

    // WebSocket handshake
    let (socket, _response) = tungstenite::client::client(&ws_url, tls_stream)?;

    Ok(socket)
}

/// Set socket to non-blocking mode for the main loop
pub fn set_socket_nonblocking(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
) -> Result<()> {
    let stream = socket.get_mut();
    let tcp_stream = stream.get_mut();
    tcp_stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    Ok(())
}

/// Set a short timeout for the setup phase so we can check for model changes frequently
pub fn set_socket_short_timeout(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
) -> Result<()> {
    let stream = socket.get_mut();
    let tcp_stream = stream.get_mut();
    // 200ms timeout allows checking for model changes 5 times per second
    tcp_stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(())
}

/// Send session setup message to configure transcription mode
pub fn send_setup_message(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
) -> Result<()> {
    // Using camelCase as per Gemini Live API documentation
    // We set responseModalities to AUDIO to satisfy the native audio model,
    // but we'll only use the inputAudioTranscription (ignore audio talkback)
    let setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", REALTIME_MODEL),
            "generationConfig": {
                "responseModalities": ["AUDIO"],  // Required for native audio model
                "thinkingConfig": {
                    "thinkingBudget": 0  // Disable thinking for lower latency
                }
            },
            "inputAudioTranscription": {}  // This is what we actually want - input transcription
        }
    });

    let msg_str = setup.to_string();
    socket.write(tungstenite::Message::Text(msg_str.into()))?;
    socket.flush()?;

    Ok(())
}

/// Send audio chunk to the WebSocket
pub fn send_audio_chunk(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
    pcm_data: &[i16],
) -> Result<()> {
    // Convert i16 samples to bytes (little-endian)
    let mut bytes = Vec::with_capacity(pcm_data.len() * 2);
    for sample in pcm_data {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    let b64_audio = general_purpose::STANDARD.encode(&bytes);

    let msg = serde_json::json!({
        "realtime_input": {
            "media_chunks": [{
                "data": b64_audio,
                "mime_type": "audio/pcm;rate=16000"
            }]
        }
    });

    socket.write(tungstenite::Message::Text(msg.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

/// Parse inputTranscription from WebSocket message (what the user said)
pub fn parse_input_transcription(msg: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        if let Some(server_content) = json.get("serverContent") {
            if let Some(input_transcription) = server_content.get("inputTranscription") {
                if let Some(text) = input_transcription.get("text").and_then(|t| t.as_str()) {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}
