//! Ollama API Integration
//! Supports local LLM inference with vision and text models

use super::client::UREQ_AGENT;
use crate::gui::locale::LocaleText;
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::{ImageBuffer, Rgba};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Cursor};

/// Ollama streaming chunk response
#[derive(Deserialize, Debug)]
pub struct OllamaStreamChunk {
    #[serde(default)]
    pub response: String,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub done: bool,
}

/// Ollama non-streaming response
#[derive(Deserialize, Debug)]
pub struct OllamaGenerateResponse {
    #[serde(default)]
    pub response: String,
}

/// Ollama model info from /api/tags
#[derive(Deserialize, Debug, Clone)]
pub struct OllamaModel {
    pub name: String,
}

/// Response from /api/tags
#[derive(Deserialize, Debug)]
pub struct OllamaTagsResponse {
    #[serde(default)]
    pub models: Vec<OllamaModel>,
}

/// Model with detected capabilities
#[derive(Clone, Debug)]
pub struct OllamaModelWithCaps {
    pub name: String,
    pub has_vision: bool,
}

/// Response from /api/show
#[derive(Deserialize, Debug)]
struct OllamaShowResponse {
    #[serde(default)]
    pub modelfile: String,
    #[serde(default)]
    pub details: OllamaModelDetails,
}

#[derive(Deserialize, Debug, Default)]
struct OllamaModelDetails {
    #[serde(default)]
    pub families: Vec<String>,
}

/// Fetch available models from Ollama
pub fn fetch_ollama_models(base_url: &str) -> Result<Vec<OllamaModel>> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));

    let resp = UREQ_AGENT
        .get(&url)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to connect to Ollama: {}", e))?;

    let tags: OllamaTagsResponse = resp
        .into_body()
        .read_json()
        .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))?;

    Ok(tags.models)
}

/// Check if a model has vision capability by querying /api/show
fn check_model_has_vision(base_url: &str, model_name: &str) -> bool {
    let url = format!("{}/api/show", base_url.trim_end_matches('/'));

    let payload = serde_json::json!({
        "name": model_name
    });

    let resp = match UREQ_AGENT.post(&url).send_json(&payload) {
        Ok(r) => r,
        Err(_) => return ollama_model_has_vision(model_name, &[], ""),
    };

    if let Ok(show_resp) = resp.into_body().read_json::<OllamaShowResponse>() {
        return ollama_model_has_vision(
            model_name,
            &show_resp.details.families,
            &show_resp.modelfile,
        );
    }

    ollama_model_has_vision(model_name, &[], "")
}

fn ollama_model_has_vision(model_name: &str, families: &[String], modelfile: &str) -> bool {
    let families_str = families.join(" ").to_lowercase();
    if families_str.contains("clip") || families_str.contains("vision") {
        return true;
    }

    let modelfile_lower = modelfile.to_lowercase();
    if modelfile_lower.contains("projector") || modelfile_lower.contains("vision") {
        return true;
    }

    let name_lower = model_name.to_lowercase();
    name_lower.contains("vision")
        || name_has_vl_token(&name_lower)
        || name_lower.contains("llava")
        || name_lower.contains("bakllava")
        || name_lower.contains("moondream")
        || name_lower.contains("minicpm-v")
}

/// Detect a `vl` vision marker as a name token rather than a loose substring.
///
/// Matches glued suffixes like `qwen2.5vl`, `qwen2vl`, `qwenvl`, plus the
/// classic `-vl` / `/vl` separators, while staying token-bounded so plain-text
/// tags (e.g. `llama3.2`, `qwen2.5-coder`) never match. A `vl` qualifies when it
/// is preceded by a digit, dot, `-`, `/`, or a letter, and followed by the end
/// of a name token (string end or one of `:`, `-`, `.`, `/`, or a digit).
fn name_has_vl_token(name_lower: &str) -> bool {
    let bytes = name_lower.as_bytes();
    let mut start = 0;
    while let Some(rel) = name_lower[start..].find("vl") {
        let idx = start + rel;
        let prev_ok = idx == 0
            || matches!(bytes[idx - 1], b'0'..=b'9' | b'.' | b'-' | b'/')
            || bytes[idx - 1].is_ascii_alphabetic();
        let after = idx + 2;
        let next_ok =
            after >= bytes.len() || matches!(bytes[after], b':' | b'-' | b'.' | b'/' | b'0'..=b'9');
        if prev_ok && next_ok {
            return true;
        }
        start = idx + 2;
    }
    false
}

/// Fetch models with their capabilities (vision/text)
pub fn fetch_ollama_models_with_caps(base_url: &str) -> Result<Vec<OllamaModelWithCaps>> {
    let models = fetch_ollama_models(base_url)?;

    let mut result = Vec::new();
    for model in models {
        let has_vision = check_model_has_vision(base_url, &model.name);
        result.push(OllamaModelWithCaps {
            name: model.name,
            has_vision,
        });
    }

    Ok(result)
}

/// Generate text with Ollama (text-only, no image)
pub fn ollama_generate_text<F>(
    base_url: &str,
    model: &str,
    prompt: &str,
    streaming_enabled: bool,
    ui_language: &str,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let url = format!("{}/api/generate", base_url.trim_end_matches('/'));

    let payload = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post(&url)
        .send_json(&payload)
        .map_err(|e| anyhow::anyhow!("Ollama API Error: {}", e))?;

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<OllamaStreamChunk>(&line) {
                Ok(chunk) => {
                    // Handle thinking tokens (qwen3 and similar models)
                    if let Some(thinking) = &chunk.thinking
                        && !thinking.is_empty()
                        && !thinking_shown
                        && !content_started
                    {
                        on_chunk(locale.model_thinking);
                        thinking_shown = true;
                    }

                    // Handle response content
                    if !chunk.response.is_empty() {
                        if !content_started && thinking_shown {
                            // Wipe thinking message on first content
                            content_started = true;
                            full_content.push_str(&chunk.response);
                            let wipe_content =
                                format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                            on_chunk(&wipe_content);
                        } else {
                            content_started = true;
                            full_content.push_str(&chunk.response);
                            on_chunk(&chunk.response);
                        }
                    }

                    if chunk.done {
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
    } else {
        let ollama_resp: OllamaGenerateResponse = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))?;

        full_content = ollama_resp.response;
        on_chunk(&full_content);
    }

    Ok(full_content)
}

/// Generate with Ollama vision model (image + text)
pub fn ollama_generate_vision<F>(
    base_url: &str,
    model: &str,
    prompt: &str,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    streaming_enabled: bool,
    ui_language: &str,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let url = format!("{}/api/generate", base_url.trim_end_matches('/'));

    // Encode image as base64 PNG
    let mut image_data = Vec::new();
    image.write_to(&mut Cursor::new(&mut image_data), image::ImageFormat::Png)?;
    let b64_image = general_purpose::STANDARD.encode(&image_data);

    let payload = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "images": [b64_image],
        "stream": streaming_enabled
    });

    let resp = UREQ_AGENT
        .post(&url)
        .send_json(&payload)
        .map_err(|e| anyhow::anyhow!("Ollama Vision API Error: {}", e))?;

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut thinking_shown = false;
        let mut content_started = false;
        let locale = LocaleText::get(ui_language);

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<OllamaStreamChunk>(&line) {
                Ok(chunk) => {
                    // Handle thinking tokens
                    if let Some(thinking) = &chunk.thinking
                        && !thinking.is_empty()
                        && !thinking_shown
                        && !content_started
                    {
                        on_chunk(locale.model_thinking);
                        thinking_shown = true;
                    }

                    // Handle response content
                    if !chunk.response.is_empty() {
                        if !content_started && thinking_shown {
                            content_started = true;
                            full_content.push_str(&chunk.response);
                            let wipe_content =
                                format!("{}{}", crate::api::WIPE_SIGNAL, full_content);
                            on_chunk(&wipe_content);
                        } else {
                            content_started = true;
                            full_content.push_str(&chunk.response);
                            on_chunk(&chunk.response);
                        }
                    }

                    if chunk.done {
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
    } else {
        let ollama_resp: OllamaGenerateResponse = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))?;

        full_content = ollama_resp.response;
        on_chunk(&full_content);
    }

    Ok(full_content)
}

#[cfg(test)]
mod tests {
    use super::ollama_model_has_vision;

    #[test]
    fn detects_vision_from_families() {
        assert!(ollama_model_has_vision(
            "custom",
            &["llama".to_string(), "clip".to_string()],
            "",
        ));
    }

    #[test]
    fn detects_vision_from_modelfile() {
        assert!(ollama_model_has_vision(
            "custom",
            &[],
            "FROM base\nPARAMETER projector mmproj.bin",
        ));
    }

    #[test]
    fn detects_vision_from_model_name_when_show_is_unhelpful() {
        assert!(ollama_model_has_vision("qwen2.5vl:7b", &[], ""));
        assert!(ollama_model_has_vision("llava:latest", &[], ""));
        assert!(ollama_model_has_vision("minicpm-v:8b", &[], ""));
    }

    #[test]
    fn keeps_plain_text_model_as_text() {
        assert!(!ollama_model_has_vision("llama3.2:latest", &[], ""));
    }
}
