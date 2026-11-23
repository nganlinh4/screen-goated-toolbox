//! WebSocket connection and communication for Gemini Live LLM API

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::DynamicImage;
use image::codecs::jpeg::JpegEncoder;
use std::time::Duration;

use super::ready_session::ReadyLiveSession;
#[cfg(not(feature = "recorder-worker"))]
use super::server_frame::parse_server_frame;
use super::setup::{LiveSetupBuilder, MediaResolution, TranscriptionMode};
use super::types::LiveInputContent;

const STILL_FRAME_STREAM_COUNT: usize = 4;
const STILL_FRAME_INTERVAL_MS: u64 = 500;
const STILL_FRAME_JPEG_QUALITY: u8 = 90;
/// Longest edge of a still frame sent to a live model. Larger captures are
/// scaled down to bound upload size; smaller ones keep every pixel, because a
/// small text selection shrunk further becomes illegible. Shared with Android
/// through `parity-fixtures/gemini-live-session/still-frame.json`.
const STILL_FRAME_MAX_LONG_EDGE: u32 = 768;

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
    let (target_width, target_height) = still_frame_size(image.width(), image.height());
    if (target_width, target_height) == (image.width(), image.height()) {
        return image;
    }
    let image = image.to_rgba8();
    DynamicImage::ImageRgba8(image::imageops::resize(
        &image,
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    ))
}

/// Scales a frame so its long edge fits the budget. Never enlarges.
fn still_frame_size(width: u32, height: u32) -> (u32, u32) {
    let long_edge = width.max(height);
    if long_edge <= STILL_FRAME_MAX_LONG_EDGE {
        return (width, height);
    }
    let scale = f64::from(STILL_FRAME_MAX_LONG_EDGE) / f64::from(long_edge);
    let fit = |edge: u32| ((f64::from(edge) * scale).round() as u32).max(1);
    (fit(width), fit(height))
}

/// Check if the message indicates setup is complete
#[cfg(not(feature = "recorder-worker"))]
pub fn is_setup_complete(msg: &str) -> bool {
    parse_server_frame(msg)
        .map(|frame| frame.setup_complete)
        .unwrap_or(false)
}

/// Check if the message contains an error
#[cfg(not(feature = "recorder-worker"))]
pub fn parse_error(msg: &str) -> Option<String> {
    parse_server_frame(msg).ok()?.error
}

#[cfg(test)]
mod still_frame_tests {
    #[test]
    fn still_frames_follow_the_shared_long_edge_budget() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/gemini-live-session/still-frame.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["maxLongEdge"].as_u64(),
            Some(u64::from(super::STILL_FRAME_MAX_LONG_EDGE))
        );
        for case in fixture["cases"].as_array().unwrap() {
            let edge = |key: &str, index: usize| case[key][index].as_u64().unwrap() as u32;
            assert_eq!(
                super::still_frame_size(edge("input", 0), edge("input", 1)),
                (edge("output", 0), edge("output", 1)),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn a_small_selection_keeps_its_pixels() {
        let image = image::DynamicImage::new_rgba8(320, 60);
        let frame = super::downscale_live_frame(image);
        assert_eq!((frame.width(), frame.height()), (320, 60));
    }
}
