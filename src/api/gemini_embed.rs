//! Gemini Embedding 2 client — multimodal `embedContent` for the Computer
//! Control conversation memory. Text and images map into a SINGLE cross-modal
//! vector space, so a typed/spoken query can match a conversation by its words
//! OR by the screenshots it contains ("the session with the Fandom page").

use crate::api::client::UREQ_AGENT;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::OnceLock;

/// The model id confirmed working at runtime, cached after the first success so
/// we don't keep probing alternates.
static RESOLVED_MODEL: OnceLock<String> = OnceLock::new();

/// Candidate Gemini Embedding 2 model ids to try, in order. `CC_EMBED_MODEL`
/// forces one; otherwise we auto-discover whether the endpoint wants the preview
/// or GA name (it has shifted over the model's lifecycle) and cache the winner.
/// Unlike `gemini-embedding-001`, Embedding 2 takes NO `taskType`.
fn model_candidates() -> Vec<String> {
    if let Ok(m) = std::env::var("CC_EMBED_MODEL") {
        return vec![m];
    }
    if let Some(m) = RESOLVED_MODEL.get() {
        return vec![m.clone()];
    }
    vec![
        "gemini-embedding-2-preview".to_string(),
        "gemini-embedding-2".to_string(),
    ]
}

/// Output vector size (override with `CC_EMBED_DIM`). 768 is the recommended
/// compact size — ample for ranking a few dozen conversations, small on disk.
/// Changing this invalidates stored vectors (cosine needs equal lengths).
fn embed_dim() -> u32 {
    std::env::var("CC_EMBED_DIM")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&d| (128..=3072).contains(&d))
        .unwrap_or(768)
}

/// One piece of the content to embed: a text span or a base64-encoded image.
pub enum EmbedPart {
    Text(String),
    /// (mime_type, base64 data) — e.g. ("image/jpeg", "...").
    Image(String, String),
}

/// Embed interleaved parts into one vector, retrying a few times (the preview
/// endpoint fails transiently). Returns the vector, or an error after retries —
/// the caller may store an empty embedding and re-embed later.
pub fn embed(parts: &[EmbedPart], api_key: &str) -> Result<Vec<f32>> {
    let json_parts: Vec<Value> = parts
        .iter()
        .map(|p| match p {
            EmbedPart::Text(t) => json!({ "text": t }),
            EmbedPart::Image(mime, data) => {
                json!({ "inlineData": { "mimeType": mime, "data": data } })
            }
        })
        .collect();
    let payload = json!({
        "content": { "parts": json_parts },
        "outputDimensionality": embed_dim(),
    });

    let mut last_err = None;
    for model in model_candidates() {
        let url =
            format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:embedContent");
        for attempt in 0..2u64 {
            match try_embed(&url, api_key, &payload) {
                Ok(v) if !v.is_empty() => {
                    let _ = RESOLVED_MODEL.set(model.clone());
                    return Ok(v);
                }
                Ok(_) => last_err = Some(anyhow::anyhow!("empty embedding")),
                Err(e) => last_err = Some(e),
            }
            std::thread::sleep(std::time::Duration::from_millis(300 * (attempt + 1)));
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("embed failed")))
}

fn try_embed(url: &str, api_key: &str, payload: &Value) -> Result<Vec<f32>> {
    let resp = UREQ_AGENT
        .post(url)
        .header("x-goog-api-key", api_key)
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let root: Value = resp
        .into_body()
        .read_json()
        .context("parse embed response")?;
    // Embedding 2: { "embeddings": [ { "values": [...] } ] }
    // Single (001): { "embedding": { "values": [...] } } — accept both.
    let values = root
        .get("embeddings")
        .and_then(|e| e.get(0))
        .and_then(|e| e.get("values"))
        .or_else(|| root.get("embedding").and_then(|e| e.get("values")))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("no embedding values in response"))?;
    Ok(values
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect())
}

/// Cosine similarity of two equal-length vectors; 0 if either is empty or the
/// lengths differ (e.g. a vector embedded at a different dimensionality).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}
