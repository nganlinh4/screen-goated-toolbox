//! WebSocket connection and communication for Gemini Live LLM API

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use native_tls::TlsStream;
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::WebSocket;

use super::types::{LiveInputContent, GEMINI_LIVE_MODEL};

/// Create TLS WebSocket connection to Gemini Live API
pub fn connect_live_websocket(api_key: &str) -> Result<WebSocket<TlsStream<TcpStream>>> {
    let ws_url = format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={}",
        api_key
    );

    let url = url::Url::parse(&ws_url)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
    let port = 443;

    use std::net::ToSocketAddrs;
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve hostname: {}", host))?;

    let tcp_stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
    tcp_stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_nodelay(true)?;

    let connector = native_tls::TlsConnector::new()?;
    let tls_stream = connector.connect(host, tcp_stream)?;

    let (socket, _response) = tungstenite::client::client(&ws_url, tls_stream)?;

    Ok(socket)
}

/// Send setup message for text-output mode
/// The native audio model REQUIRES responseModalities: ["AUDIO"]
/// but we can still extract text from the response and ignore the audio
pub fn send_live_setup(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    system_instruction: Option<&str>,
    _enable_thinking: bool, // Thinking not supported on native audio model
) -> Result<()> {
    // Native audio model requires AUDIO modality - we'll ignore the audio and extract text
    // We also request inputAudioTranscription to get text responses
    let mut setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", GEMINI_LIVE_MODEL),
            "generationConfig": {
                "responseModalities": ["AUDIO"],  // Required for native audio model
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": "Aoede"  // Default voice (we'll ignore the audio)
                        }
                    }
                },
                "thinkingConfig": {
                    "thinkingBudget": 0  // Disable thinking for lower latency
                }
            },
            "outputAudioTranscription": {}  // This gives us text transcription of the audio output
        }
    });

    // Add system instruction if provided
    if let Some(instruction) = system_instruction {
        // Enforce super fast speed in system prompt as requested
        let speed_instruction =
            "IMPORTANT: You must respond as fast as possible. Be concise and direct.";

        let final_instruction = if instruction.trim().is_empty() {
            speed_instruction.to_string()
        } else {
            format!("{} {}", instruction, speed_instruction)
        };

        setup["setup"]["systemInstruction"] = serde_json::json!({
            "parts": [{
                "text": final_instruction
            }]
        });
    }

    let msg_str = setup.to_string();
    socket.write(tungstenite::Message::Text(msg_str.into()))?;
    socket.flush()?;

    Ok(())
}

/// Send content to the model (text, image, or audio)
pub fn send_live_content(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    content: &LiveInputContent,
) -> Result<()> {
    let parts = match content {
        LiveInputContent::Text(text) => {
            serde_json::json!([{
                "text": text
            }])
        }
        LiveInputContent::TextWithImage {
            text,
            image_data,
            mime_type,
        } => {
            let b64_image = general_purpose::STANDARD.encode(image_data);
            serde_json::json!([
                { "text": text },
                {
                    "inlineData": {
                        "mimeType": mime_type,
                        "data": b64_image
                    }
                }
            ])
        }
        LiveInputContent::TextWithAudio { text, audio_data } => {
            let b64_audio = general_purpose::STANDARD.encode(audio_data);
            serde_json::json!([
                { "text": text },
                {
                    "inlineData": {
                        "mimeType": "audio/pcm;rate=16000",
                        "data": b64_audio
                    }
                }
            ])
        }
        LiveInputContent::AudioOnly(audio_data) => {
            let b64_audio = general_purpose::STANDARD.encode(audio_data);
            serde_json::json!([{
                "inlineData": {
                    "mimeType": "audio/pcm;rate=16000",
                    "data": b64_audio
                }
            }])
        }
    };

    let msg = serde_json::json!({
        "clientContent": {
            "turns": [{
                "role": "user",
                "parts": parts
            }],
            "turnComplete": true
        }
    });

    socket.write(tungstenite::Message::Text(msg.to_string().into()))?;
    socket.flush()?;

    Ok(())
}

/// Parse text content from WebSocket message
/// The native audio model returns audio, but with outputAudioTranscription enabled,
/// we get text transcription of what it "said" as well
/// Returns (text_chunk, is_thought, is_turn_complete)
pub fn parse_live_response(msg: &str) -> (Option<String>, bool, bool) {
    let mut text_chunk = None;
    let is_thought = false; // Native audio model doesn't support thinking
    let mut is_turn_complete = false;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        if let Some(server_content) = json.get("serverContent") {
            // Check for turn complete
            if let Some(tc) = server_content.get("turnComplete") {
                if tc.as_bool().unwrap_or(false) {
                    is_turn_complete = true;
                }
            }

            // Check for generationComplete (faster signal)
            if let Some(gc) = server_content.get("generationComplete") {
                if gc.as_bool().unwrap_or(false) {
                    is_turn_complete = true;
                }
            }

            // Check for outputTranscription - this is the text we want!
            // Note: The field name is "outputTranscription" (not "outputAudioTranscription")
            // Don't trim - leading spaces are intentional word separators
            if let Some(transcription) = server_content.get("outputTranscription") {
                if let Some(text) = transcription.get("text").and_then(|t| t.as_str()) {
                    // Only skip if it's purely whitespace (like just "\n")
                    if !text.chars().all(char::is_whitespace) {
                        text_chunk = Some(text.to_string());
                    }
                }
            }

            // Also check model turn for any direct text (fallback)
            if text_chunk.is_none() {
                if let Some(model_turn) = server_content.get("modelTurn") {
                    if let Some(parts) = model_turn.get("parts").and_then(|p| p.as_array()) {
                        for part in parts {
                            // Extract text if present
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    text_chunk = Some(text.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    (text_chunk, is_thought, is_turn_complete)
}

/// Check if the message indicates setup is complete
pub fn is_setup_complete(msg: &str) -> bool {
    msg.contains("setupComplete")
}

/// Check if the message contains an error
pub fn parse_error(msg: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        if let Some(error) = json.get("error") {
            if let Some(message) = error.get("message").and_then(|m| m.as_str()) {
                return Some(message.to_string());
            }
            return Some(error.to_string());
        }
    }
    None
}
