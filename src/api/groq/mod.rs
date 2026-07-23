//! Shared Groq capabilities. Provider behavior lives here so callers do not
//! grow model-name conditionals or implement incompatible retry policies.

pub mod batch;

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::api::client::{UREQ_AGENT, record_groq_json_usage, record_usage_simple};

pub const CHAT_COMPLETIONS_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ServiceTier {
    #[default]
    OnDemand,
    /// Paid, fail-fast background capacity. HTTP 498 is retried with bounded jitter.
    Flex,
}

pub fn supports_strict_structured_output(model: &str) -> bool {
    model == "openai/gpt-oss-120b"
}

/// Select the strongest schema mode the requested model officially supports.
pub fn structured_response_format(model: &str, name: &str, schema: Value) -> Value {
    if supports_strict_structured_output(model) {
        serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": name,
                "strict": true,
                "schema": schema
            }
        })
    } else {
        serde_json::json!({ "type": "json_object" })
    }
}

/// Sends an arbitrary chat payload, including caller-defined local tool schemas.
/// Reasoning fields are deliberately left untouched so Groq uses model defaults.
pub fn send_chat_completion(
    api_key: &str,
    mut payload: Value,
    stats_key: &str,
    tier: ServiceTier,
) -> Result<Value> {
    if tier == ServiceTier::Flex {
        payload["service_tier"] = Value::String("flex".to_string());
    }

    let max_attempts = if tier == ServiceTier::Flex { 4 } else { 1 };
    for attempt in 0..max_attempts {
        match UREQ_AGENT
            .post(CHAT_COMPLETIONS_URL)
            .header("Authorization", &format!("Bearer {api_key}"))
            .send_json(&payload)
        {
            Ok(response) => {
                record_usage_simple(response.headers(), stats_key);
                let root: Value = response
                    .into_body()
                    .read_json()
                    .context("Parse Groq chat response")?;
                record_groq_json_usage(stats_key, &root);
                return Ok(root);
            }
            Err(ureq::Error::StatusCode(498)) if attempt + 1 < max_attempts => {
                let delay = flex_retry_delay(attempt);
                crate::log_info!(
                    "[Groq] flex capacity unavailable; retry {}/{} in {}ms",
                    attempt + 1,
                    max_attempts - 1,
                    delay.as_millis()
                );
                std::thread::sleep(delay);
            }
            Err(error) => return Err(anyhow!("Groq chat request failed: {error}")),
        }
    }
    Err(anyhow!("Groq Flex capacity remained unavailable"))
}

fn flex_retry_delay(attempt: usize) -> Duration {
    let cap_ms = 8_000_u64;
    let base_ms = 500_u64.saturating_mul(1_u64 << attempt.min(4));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter_ms = nanos % (base_ms / 2 + 1);
    Duration::from_millis((base_ms + jitter_ms).min(cap_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_schema_is_used_only_where_supported() {
        let schema = serde_json::json!({"type": "object"});
        let strict = structured_response_format("openai/gpt-oss-120b", "result", schema.clone());
        let generic = structured_response_format("future-vision-model", "result", schema.clone());
        let qwen = structured_response_format("qwen/qwen3.6-27b", "result", schema);
        assert_eq!(strict["json_schema"]["strict"], true);
        assert_eq!(generic["type"], "json_object");
        assert_eq!(qwen["type"], "json_object");
    }

    #[test]
    fn flex_backoff_is_bounded() {
        for attempt in 0..10 {
            assert!(flex_retry_delay(attempt) <= Duration::from_secs(8));
        }
    }
}
