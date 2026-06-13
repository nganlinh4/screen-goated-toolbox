//! WebSocket connection and communication for Gemini Live API

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use std::io;
use std::net::TcpStream;
use std::time::Duration;

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

/// Send session setup message to configure transcription mode
pub fn send_setup_message(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
    model: &str,
) -> Result<()> {
    let mut generation_config = serde_json::json!({
        "responseModalities": ["AUDIO"],
        "mediaResolution": "MEDIA_RESOLUTION_LOW",
    });

    generation_config["thinkingConfig"] = serde_json::json!({
        "thinkingBudget": 0
    });

    let setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", model),
            "generationConfig": generation_config,
            "inputAudioTranscription": {}
        }
    });

    let msg_str = setup.to_string();
    socket.write(tungstenite::Message::Text(msg_str.into()))?;
    socket.flush()?;

    Ok(())
}

/// Send session setup for Gemini Live Translate models.
pub fn send_live_translate_setup_message(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
    model: &str,
    target_language: &str,
) -> Result<()> {
    let setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", model),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "translationConfig": {
                    "targetLanguageCode": live_translate_target_language_code(target_language),
                    "echoTargetLanguage": true
                }
            },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {}
        }
    });

    socket.write(tungstenite::Message::Text(setup.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

pub fn send_audio_stream_end(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>,
) -> Result<()> {
    let msg = serde_json::json!({
        "realtimeInput": {
            "audioStreamEnd": true
        }
    });

    socket.write(tungstenite::Message::Text(msg.to_string().into()))?;
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
        "realtimeInput": {
            "audio": {
                "data": b64_audio,
                "mimeType": "audio/pcm;rate=16000"
            }
        }
    });

    socket.write(tungstenite::Message::Text(msg.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

/// Parse inputTranscription from WebSocket message (what the user said)
pub fn parse_input_transcription(msg: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg)
        && let Some(server_content) = json.get("serverContent")
        && let Some(input_transcription) = server_content.get("inputTranscription")
        && let Some(text) = input_transcription.get("text").and_then(|t| t.as_str())
    {
        return Some(text.to_string());
    }
    None
}

/// Parse outputTranscription from WebSocket message (what the model said).
pub fn parse_output_transcription(msg: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg)
        && let Some(server_content) = json.get("serverContent")
        && let Some(output_transcription) = server_content.get("outputTranscription")
        && let Some(text) = output_transcription.get("text").and_then(|t| t.as_str())
    {
        return Some(text.to_string());
    }
    None
}

pub fn live_translate_target_language_code(language: &str) -> String {
    let trimmed = language.trim();
    if trimmed.is_empty() {
        return "en".to_string();
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "chinese"
        | "chinese (simplified)"
        | "simplified chinese"
        | "zh"
        | "zh-cn"
        | "zh-hans"
        | "zh_hans" => return "zh-Hans".to_string(),
        "chinese (traditional)" | "traditional chinese" | "zh-tw" | "zh-hant" | "zh_hant" => {
            return "zh-Hant".to_string();
        }
        "portuguese (brazil)" | "brazilian portuguese" | "pt-br" | "pt_br" => {
            return "pt-BR".to_string();
        }
        "portuguese (portugal)" | "european portuguese" | "pt-pt" | "pt_pt" => {
            return "pt-PT".to_string();
        }
        "filipino" | "tagalog" => return "fil".to_string(),
        "norwegian" => return "no".to_string(),
        code if is_bcp47_like(code) => return normalize_bcp47_code(trimmed),
        _ => {}
    }

    isolang::Language::from_name(trimmed)
        .map(|language| language.to_639_1().unwrap_or_else(|| language.to_639_3()))
        .map(str::to_string)
        .unwrap_or_else(|| "en".to_string())
}

fn is_bcp47_like(value: &str) -> bool {
    let mut parts = value.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    language.len() >= 2
        && language.len() <= 3
        && language.chars().all(|ch| ch.is_ascii_lowercase())
        && parts.all(|part| {
            !part.is_empty()
                && part.len() <= 8
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

fn normalize_bcp47_code(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "zh-hans" => "zh-Hans".to_string(),
        "zh-hant" => "zh-Hant".to_string(),
        "pt-br" => "pt-BR".to_string(),
        "pt-pt" => "pt-PT".to_string(),
        value => value.to_string(),
    }
}
