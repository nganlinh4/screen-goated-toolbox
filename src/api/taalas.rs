//! Taalas / chatjimmy.ai API client (Llama 3.1 8B on HC1 silicon, ~17,000 tok/s).
//!
//! Single shared entry point for all Taalas calls (translate, refine, realtime).

use crate::api::client::UREQ_AGENT;
use std::io::Read;

const ENDPOINT: &str = "https://chatjimmy.ai/api/chat";
const MODEL: &str = "llama3.1-8B";
const TOP_K: u8 = 8;
const STATS_MARKER: &str = "<|stats|>";

/// Send a prompt to chatjimmy.ai and return the clean response text.
///
/// Returns `None` on HTTP error or blank response.
/// The response is buffered fully (near-instant at ~17k tok/s) then stats are stripped.
pub fn generate(prompt: &str) -> Option<String> {
    let payload = serde_json::json!({
        "messages": [{ "role": "user", "content": prompt }],
        "chatOptions": {
            "selectedModel": MODEL,
            "topK": TOP_K
        }
    });

    let resp = UREQ_AGENT
        .post(ENDPOINT)
        .header("Content-Type", "application/json")
        .send_json(payload)
        .ok()?;

    let mut reader = resp.into_body().into_reader();
    let mut buf = [0u8; 8192];
    let mut raw = String::new();
    loop {
        let n = reader.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        raw.push_str(&String::from_utf8_lossy(&buf[..n]));
    }

    let clean = match raw.find(STATS_MARKER) {
        Some(i) => raw[..i].trim(),
        None => raw.trim(),
    };

    if clean.is_empty() {
        None
    } else {
        Some(clean.to_string())
    }
}
