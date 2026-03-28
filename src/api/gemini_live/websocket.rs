//! WebSocket connection and communication for Gemini Live LLM API

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::DynamicImage;
use image::codecs::jpeg::JpegEncoder;
use native_tls::TlsStream;
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::WebSocket;

use super::types::LiveInputContent;

const STILL_FRAME_STREAM_COUNT: usize = 4;
const STILL_FRAME_INTERVAL_MS: u64 = 500;
const STILL_FRAME_JPEG_QUALITY: u8 = 90;

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

/// Send setup message for text-output mode.
/// We request native AUDIO responses and consume `outputAudioTranscription`
/// so both Gemini 2.5 native-audio and Gemini 3.1 Flash Live can be used
/// through the same text-first interface.
pub fn send_live_setup(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    model: &str,
    system_instruction: Option<&str>,
    enable_thinking: bool,
) -> Result<()> {
    let uses_live_3_1 = model == crate::model_config::GEMINI_LIVE_API_MODEL_3_1;
    let mut generation_config = serde_json::json!({
        "responseModalities": ["AUDIO"],
        "mediaResolution": "MEDIA_RESOLUTION_LOW",
        "speechConfig": {
            "voiceConfig": {
                "prebuiltVoiceConfig": {
                    "voiceName": "Aoede"
                }
            }
        }
    });

    if !enable_thinking {
        generation_config["thinkingConfig"] = serde_json::json!({
            "thinkingBudget": 0
        });
    } else if uses_live_3_1 {
        generation_config["thinkingConfig"] = serde_json::json!({
            "thinkingLevel": "minimal"
        });
    } else {
        generation_config["thinkingConfig"] = serde_json::json!({
            "includeThoughts": true
        });
    }

    let mut setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", model),
            "generationConfig": generation_config,
            "outputAudioTranscription": {}
        }
    });

    if let Some(instruction) = system_instruction {
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

    send_live_json(socket, setup)
}

/// Send content to the model.
/// Gemini 3.1 expects `realtimeInput` for live text turns. We keep audio streaming
/// on the same path and use `video` for image inputs.
pub fn send_live_content(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    content: &LiveInputContent,
) -> Result<()> {
    match content {
        LiveInputContent::Text(text) => {
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "text": text
                    }
                }),
            )?;
        }
        LiveInputContent::TextWithImage {
            text,
            image_data,
            mime_type,
        } => {
            let (frame_bytes, frame_mime_type) = build_live_still_frame(image_data, mime_type);
            let b64_frame = general_purpose::STANDARD.encode(frame_bytes);
            for frame_idx in 0..STILL_FRAME_STREAM_COUNT {
                send_live_json(
                    socket,
                    serde_json::json!({
                        "realtimeInput": {
                            "video": {
                                "mimeType": frame_mime_type.clone(),
                                "data": b64_frame.clone()
                            }
                        }
                    }),
                )?;
                if frame_idx + 1 < STILL_FRAME_STREAM_COUNT {
                    std::thread::sleep(Duration::from_millis(STILL_FRAME_INTERVAL_MS));
                }
            }
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "text": text
                    }
                }),
            )?;
        }
        LiveInputContent::TextWithAudio { text, audio_data } => {
            let b64_audio = general_purpose::STANDARD.encode(audio_data);
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "text": text
                    }
                }),
            )?;
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "audio": {
                            "mimeType": "audio/pcm;rate=16000",
                            "data": b64_audio
                        }
                    }
                }),
            )?;
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "audioStreamEnd": true
                    }
                }),
            )?;
        }
        LiveInputContent::AudioOnly(audio_data) => {
            let b64_audio = general_purpose::STANDARD.encode(audio_data);
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "audio": {
                            "mimeType": "audio/pcm;rate=16000",
                            "data": b64_audio
                        }
                    }
                }),
            )?;
            send_live_json(
                socket,
                serde_json::json!({
                    "realtimeInput": {
                        "audioStreamEnd": true
                    }
                }),
            )?;
        }
    }

    Ok(())
}

fn build_live_still_frame(image_data: &[u8], mime_type: &str) -> (Vec<u8>, String) {
    if let Ok(dynamic) = image::load_from_memory(image_data) {
        let resized = downscale_live_frame(dynamic);
        let mut jpeg_bytes = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_bytes, STILL_FRAME_JPEG_QUALITY);
        if encoder.encode_image(&resized).is_ok() {
            return (jpeg_bytes, "image/jpeg".to_string());
        }
    }

    (image_data.to_vec(), mime_type.to_string())
}

fn downscale_live_frame(image: DynamicImage) -> DynamicImage {
    let image = image.to_rgba8();
    let target_width = (image.width() / 4).max(1);
    let target_height = (image.height() / 4).max(1);
    DynamicImage::ImageRgba8(image::imageops::resize(
        &image,
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    ))
}

pub fn set_live_read_timeout(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    timeout: Duration,
) -> Result<()> {
    let stream = socket.get_mut();
    let tcp_stream = stream.get_mut();
    tcp_stream.set_read_timeout(Some(timeout))?;
    Ok(())
}

/// Parse text content from WebSocket message.
/// Returns `(text_chunk, is_thought, is_turn_complete)`.
pub fn parse_live_response(msg: &str) -> (Option<String>, bool, bool) {
    let mut text_chunks: Vec<String> = Vec::new();
    let mut thought_chunks: Vec<String> = Vec::new();
    let mut is_turn_complete = false;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg)
        && let Some(server_content) = json.get("serverContent")
    {
        if let Some(tc) = server_content.get("turnComplete")
            && tc.as_bool().unwrap_or(false)
        {
            is_turn_complete = true;
        }

        if let Some(gc) = server_content.get("generationComplete")
            && gc.as_bool().unwrap_or(false)
        {
            is_turn_complete = true;
        }

        if let Some(transcription) = server_content.get("outputTranscription")
            && let Some(text) = transcription.get("text").and_then(|t| t.as_str())
            && !text.chars().all(char::is_whitespace)
        {
            text_chunks.push(text.to_string());
        }

        if let Some(model_turn) = server_content.get("modelTurn")
            && let Some(parts) = model_turn.get("parts").and_then(|p| p.as_array())
        {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str())
                    && !text.is_empty()
                {
                    if part
                        .get("thought")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        thought_chunks.push(text.to_string());
                    } else {
                        text_chunks.push(text.to_string());
                    }
                }
            }
        }
    }

    if !text_chunks.is_empty() {
        return (Some(text_chunks.concat()), false, is_turn_complete);
    }
    if !thought_chunks.is_empty() {
        return (Some(thought_chunks.concat()), true, is_turn_complete);
    }

    (None, false, is_turn_complete)
}

/// Check if the message indicates setup is complete
pub fn is_setup_complete(msg: &str) -> bool {
    msg.contains("setupComplete")
}

/// Check if the message contains an error
pub fn parse_error(msg: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg)
        && let Some(error) = json.get("error")
    {
        if let Some(message) = error.get("message").and_then(|m| m.as_str()) {
            return Some(message.to_string());
        }
        return Some(error.to_string());
    }
    None
}

fn send_live_json(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    payload: serde_json::Value,
) -> Result<()> {
    socket.write(tungstenite::Message::Text(payload.to_string().into()))?;
    socket.flush()?;
    Ok(())
}
