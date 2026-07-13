//! WebSocket connection and communication for Gemini Live LLM API

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::DynamicImage;
use image::codecs::jpeg::JpegEncoder;
use std::time::Duration;

use super::ready_session::ReadyLiveSession;
use super::server_frame::parse_server_frame;
use super::setup::{LiveSetupBuilder, MediaResolution, TranscriptionMode};
use super::types::LiveInputContent;

const STILL_FRAME_STREAM_COUNT: usize = 4;
const STILL_FRAME_INTERVAL_MS: u64 = 500;
const STILL_FRAME_JPEG_QUALITY: u8 = 90;

/// Build the text-first setup envelope without coupling setup construction to
/// a raw socket. Setup-gated transports use this before promoting a connected
/// socket into a ready session.
pub fn build_live_setup(
    model: &str,
    system_instruction: Option<&str>,
    _enable_thinking: bool,
) -> serde_json::Value {
    let mut builder = LiveSetupBuilder::new(model)
        .media_resolution(MediaResolution::Low)
        .voice("Aoede")
        .transcription(TranscriptionMode::Output);

    if let Some(instruction) = system_instruction {
        let speed_instruction =
            "IMPORTANT: You must respond as fast as possible. Be concise and direct.";

        let final_instruction = if instruction.trim().is_empty() {
            speed_instruction.to_string()
        } else {
            format!("{} {}", instruction, speed_instruction)
        };

        builder = builder.system_instruction(&final_instruction);
    }

    builder.build()
}

/// Send content to the model.
/// Gemini 3.1 expects `realtimeInput` for live text turns. We keep audio streaming
/// on the same path and use `video` for image inputs.
pub fn send_live_content(session: &mut ReadyLiveSession, content: &LiveInputContent) -> Result<()> {
    match content {
        LiveInputContent::Text(text) => {
            session.send_json(&serde_json::json!({
                "realtimeInput": {
                    "text": text
                }
            }))?;
        }
        LiveInputContent::TextWithImage {
            text,
            image_data,
            mime_type,
        } => {
            let (frame_bytes, frame_mime_type) = build_live_still_frame(image_data, mime_type);
            let b64_frame = general_purpose::STANDARD.encode(frame_bytes);
            for frame_idx in 0..STILL_FRAME_STREAM_COUNT {
                session.send_json(&serde_json::json!({
                    "realtimeInput": {
                        "video": {
                            "mimeType": frame_mime_type.clone(),
                            "data": b64_frame.clone()
                        }
                    }
                }))?;
                if frame_idx + 1 < STILL_FRAME_STREAM_COUNT {
                    std::thread::sleep(Duration::from_millis(STILL_FRAME_INTERVAL_MS));
                }
            }
            session.send_json(&serde_json::json!({
                "realtimeInput": {
                    "text": text
                }
            }))?;
        }
        LiveInputContent::TextWithAudio { text, audio_data } => {
            session.send_json(&serde_json::json!({
                "realtimeInput": {
                    "text": text
                }
            }))?;
            session.send_audio_bytes(audio_data, 16_000)?;
            session.end_audio_stream()?;
        }
        LiveInputContent::AudioOnly(audio_data) => {
            session.send_audio_bytes(audio_data, 16_000)?;
            session.end_audio_stream()?;
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

/// Check if the message indicates setup is complete
pub fn is_setup_complete(msg: &str) -> bool {
    parse_server_frame(msg)
        .map(|frame| frame.setup_complete)
        .unwrap_or(false)
}

/// Check if the message contains an error
pub fn parse_error(msg: &str) -> Option<String> {
    parse_server_frame(msg).ok()?.error
}
