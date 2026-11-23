//! Ollama API Integration
//! Supports local LLM inference with vision and text models

use super::client::{UREQ_AGENT, UREQ_STREAM_AGENT};
use crate::gui::locale::LocaleText;
use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};

/// Ollama streaming chunk response
#[derive(Deserialize, Debug)]
pub struct OllamaStreamChunk {
    #[serde(default)]
    pub response: String,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub error: Option<String>,
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
        .map_err(|e| anyhow!("Failed to connect to Ollama: {}", e))?;

    let tags: OllamaTagsResponse = resp
        .into_body()
        .read_json()
        .map_err(|e| anyhow!("Failed to parse Ollama response: {}", e))?;

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

fn ensure_active(cancel_token: Option<&AtomicBool>) -> Result<()> {
    if cancel_token.is_some_and(|token| token.load(Ordering::SeqCst)) {
        bail!("Ollama request cancelled");
    }
    Ok(())
}

/// Local models on a CPU can take minutes to load and generate, so a stream is
/// bounded by its idle deadlines rather than the unary end-to-end cap.
fn generate_agent(streaming: bool) -> &'static ureq::Agent {
    if streaming {
        &UREQ_STREAM_AGENT
    } else {
        &UREQ_AGENT
    }
}

fn read_generate_response<F>(
    resp: ureq::http::Response<ureq::Body>,
    streaming: bool,
    ui_language: &str,
    cancel_token: Option<&AtomicBool>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let full_content = if streaming {
        read_generate_stream(
            BufReader::new(resp.into_body().into_reader()),
            ui_language,
            cancel_token,
            on_chunk,
        )?
    } else {
        let ollama_resp: OllamaGenerateResponse = resp
            .into_body()
            .read_json()
            .map_err(|e| anyhow!("Failed to parse Ollama response: {}", e))?;
        ensure_active(cancel_token)?;
        on_chunk(&ollama_resp.response);
        ollama_resp.response
    };
    ensure_active(cancel_token)?;
    Ok(full_content)
}

/// Reads Ollama's NDJSON generate stream. Only a stream that reaches `done`
/// succeeds; an `error` line, an undecodable line or an early end is a
/// provider failure so the retry chain can fall back instead of keeping a
/// partial answer.
fn read_generate_stream<F>(
    reader: impl BufRead,
    ui_language: &str,
    cancel_token: Option<&AtomicBool>,
    on_chunk: &mut F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let locale = LocaleText::get(ui_language);
    let mut full_content = String::new();
    let mut thinking_shown = false;
    let mut content_started = false;
    for line in reader.lines() {
        let line = line.map_err(|error| {
            ensure_active(cancel_token)
                .err()
                .unwrap_or_else(|| error.into())
        })?;
        ensure_active(cancel_token)?;
        if line.trim().is_empty() {
            continue;
        }
        let chunk = serde_json::from_str::<OllamaStreamChunk>(&line).map_err(|error| {
            anyhow!("PROVIDER_RESPONSE_INVALID:Malformed Ollama stream line: {error}")
        })?;
        if let Some(error) = chunk.error {
            bail!("PROVIDER_RESPONSE_INVALID:Ollama error: {error}");
        }
        // Local reasoning tokens are generated output too, so they renew the
        // idle window; only an empty keepalive line does not.
        if !chunk.response.is_empty() || chunk.thinking.as_deref().is_some_and(|t| !t.is_empty()) {
            note_output();
        }
        // Thinking tokens (qwen3 and similar models) show a placeholder that
        // the first content chunk wipes.
        if chunk
            .thinking
            .as_deref()
            .is_some_and(|text| !text.is_empty())
            && !thinking_shown
            && !content_started
        {
            on_chunk(locale.global_settings.model_thinking);
            ensure_active(cancel_token)?;
            thinking_shown = true;
        }
        if !chunk.response.is_empty() {
            full_content.push_str(&chunk.response);
            if !content_started && thinking_shown {
                on_chunk(&format!("{}{}", crate::api::WIPE_SIGNAL, full_content));
            } else {
                on_chunk(&chunk.response);
            }
            content_started = true;
        }
        if chunk.done {
            return Ok(full_content);
        }
    }
    bail!("PROVIDER_RESPONSE_INVALID:Ollama stream ended before completion")
}

pub fn ollama_generate_text_cancellable<F>(
    base_url: &str,
    model: &str,
    prompt: &str,
    streaming_enabled: bool,
    ui_language: &str,
    cancel_token: Option<&AtomicBool>,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    ensure_active(cancel_token)?;
    let url = format!("{}/api/generate", base_url.trim_end_matches('/'));

    let payload = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": streaming_enabled
    });

    let resp = generate_agent(streaming_enabled)
        .post(&url)
        .send_json(&payload)
        .map_err(|e| {
            ensure_active(cancel_token)
                .err()
                .unwrap_or_else(|| anyhow!("Ollama API Error: {}", e))
        })?;
    ensure_active(cancel_token)?;

    read_generate_response(
        resp,
        streaming_enabled,
        ui_language,
        cancel_token,
        &mut on_chunk,
    )
}

/// Renews the request's output deadline. The recorder worker has no
/// request-scoped deadlines, so there it is a no-op.
fn note_output() {
    #[cfg(not(feature = "recorder-worker"))]
    crate::api::client::first_token::received();
}

/// Text generation for a retry chain attempt.
#[cfg(not(feature = "recorder-worker"))]
pub struct OllamaChainTextRequest<'a> {
    pub base_url: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    pub streaming_enabled: bool,
    pub ui_language: &'a str,
    pub cancel_token: &'a Option<std::sync::Arc<AtomicBool>>,
    pub request_timeout: Option<crate::api::client::RequestTimeouts>,
}

/// Runs a streaming attempt under the chain's first-output and renewable idle
/// deadlines, which also observe cancellation while a read is blocked, so a
/// stalled or still-loading local model hands the chain back control instead
/// of holding it for the shared agent's two-minute limits.
#[cfg(not(feature = "recorder-worker"))]
pub fn ollama_generate_text_in_chain<F>(
    request: OllamaChainTextRequest<'_>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let _deadline = crate::api::client::first_token::FirstTokenGuard::new(
        request.streaming_enabled,
        request.request_timeout,
        request.cancel_token,
    );
    ollama_generate_text_cancellable(
        request.base_url,
        request.model,
        request.prompt,
        request.streaming_enabled,
        request.ui_language,
        request.cancel_token.as_deref(),
        on_chunk,
    )
}

#[cfg(not(feature = "recorder-worker"))]
#[path = "ollama_vision.rs"]
mod vision;
#[cfg(not(feature = "recorder-worker"))]
pub use vision::{OllamaVisionRequest, ollama_generate_vision};

#[cfg(test)]
mod tests {
    use super::{ollama_model_has_vision, read_generate_stream};

    fn read(body: &str) -> (anyhow::Result<String>, Vec<String>) {
        let mut chunks = Vec::new();
        let result = read_generate_stream(body.as_bytes(), "en", None, &mut |chunk: &str| {
            chunks.push(chunk.to_string())
        });
        (result, chunks)
    }

    #[test]
    fn completed_stream_returns_the_whole_answer() {
        let (result, chunks) = read(concat!(
            r#"{"response":"Hel"}"#,
            "\n\n",
            r#"{"response":"lo","done":true}"#,
            "\n"
        ));
        assert_eq!(result.unwrap(), "Hello");
        assert_eq!(chunks, ["Hel", "lo"]);
    }

    #[test]
    fn error_lines_and_unfinished_streams_are_provider_failures() {
        for body in [
            concat!(
                r#"{"response":"partial"}"#,
                "\n",
                r#"{"error":"model requires more system memory"}"#,
                "\n"
            ),
            concat!(r#"{"response":"partial"}"#, "\n"),
            concat!(
                r#"{"response":"partial"}"#,
                "\nnot json\n",
                r#"{"done":true}"#,
                "\n"
            ),
        ] {
            let error = read(body).0.unwrap_err().to_string();
            assert!(
                error.starts_with("PROVIDER_RESPONSE_INVALID:"),
                "{body}: {error}"
            );
        }
    }

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

#[cfg(test)]
#[path = "ollama_cancellation_tests.rs"]
mod cancellation_tests;
#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "ollama_chain_deadline_tests.rs"]
mod chain_deadline_tests;
