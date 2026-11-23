//! Image-and-text generation with an Ollama vision model. Only the host app
//! sends images; the recorder worker generates text alone.

use super::{ensure_active, generate_agent, read_generate_response};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use image::{ImageBuffer, Rgba};
use std::io::Cursor;
use std::sync::atomic::AtomicBool;

/// Generate with Ollama vision model (image + text)
pub struct OllamaVisionRequest<'a> {
    pub base_url: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    pub image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    pub streaming_enabled: bool,
    pub ui_language: &'a str,
    pub cancel_token: Option<&'a AtomicBool>,
}

pub fn ollama_generate_vision<F>(
    request: OllamaVisionRequest<'_>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let OllamaVisionRequest {
        base_url,
        model,
        prompt,
        image,
        streaming_enabled,
        ui_language,
        cancel_token,
    } = request;
    ensure_active(cancel_token)?;
    let url = format!("{}/api/generate", base_url.trim_end_matches('/'));

    // Encode image as base64 PNG
    let mut image_data = Vec::new();
    image.write_to(&mut Cursor::new(&mut image_data), image::ImageFormat::Png)?;
    let b64_image = general_purpose::STANDARD.encode(&image_data);
    ensure_active(cancel_token)?;

    let payload = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "images": [b64_image],
        "stream": streaming_enabled
    });

    let resp = generate_agent(streaming_enabled)
        .post(&url)
        .send_json(&payload)
        .map_err(|e| anyhow!("Ollama Vision API Error: {}", e))?;
    ensure_active(cancel_token)?;

    read_generate_response(
        resp,
        streaming_enabled,
        ui_language,
        cancel_token,
        &mut on_chunk,
    )
}
