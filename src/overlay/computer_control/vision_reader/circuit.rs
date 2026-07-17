//! Short-lived model cooldowns for auxiliary vision calls.
//!
//! Provider clients already handle a small `retry-after` inside one request.
//! This circuit prevents later calls in the same app session from hammering a
//! model that has just reported an exhausted quota.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_COOLDOWN_SECS: u64 = 300;

static RATE_LIMITED_UNTIL: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static TEXT_OVERSIZE_AT_OR_ABOVE: LazyLock<Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cooldown_duration() -> Duration {
    let seconds = std::env::var("VISION_RATE_LIMIT_COOLDOWN_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_COOLDOWN_SECS)
        .clamp(1, 86_400);
    Duration::from_secs(seconds)
}

pub(super) fn is_rate_limit_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http 429")
        || lower.contains("status code 429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
}

pub(super) fn cool_down(model_id: &str) {
    if let Ok(mut state) = RATE_LIMITED_UNTIL.lock() {
        state.insert(model_id.to_string(), Instant::now() + cooldown_duration());
    }
}

pub(super) fn remaining(model_id: &str) -> Option<Duration> {
    let now = Instant::now();
    let mut state = RATE_LIMITED_UNTIL.lock().ok()?;
    state.retain(|_, until| *until > now);
    state
        .get(model_id)
        .map(|until| until.saturating_duration_since(now))
}

pub(super) fn is_oversize_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http 413")
        || lower.contains("status code 413")
        || lower.contains("payload too large")
        || lower.contains("request entity too large")
        || lower.contains("request too large")
        || lower.contains("please reduce the length of the messages or completion")
        || lower.contains("maximum context length")
        || lower.contains("context length exceeded")
        || lower.contains("input too long")
}

pub(super) fn learn_text_oversize(model_id: &str, input_bytes: usize) {
    if input_bytes == 0 {
        return;
    }
    if let Ok(mut state) = TEXT_OVERSIZE_AT_OR_ABOVE.lock() {
        state
            .entry(model_id.to_string())
            .and_modify(|known| *known = (*known).min(input_bytes))
            .or_insert(input_bytes);
    }
}

pub(super) fn rejects_text_size(model_id: &str, input_bytes: usize) -> Option<usize> {
    let state = TEXT_OVERSIZE_AT_OR_ABOVE.lock().ok()?;
    state
        .get(model_id)
        .copied()
        .filter(|threshold| input_bytes >= *threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_structured_and_plain_rate_limits() {
        assert!(is_rate_limit_error(
            "Groq vision API HTTP 429: TPM exhausted"
        ));
        assert!(is_rate_limit_error("quota exceeded for this model"));
        assert!(!is_rate_limit_error("HTTP 503 temporarily unavailable"));
    }

    #[test]
    fn recognizes_only_explicit_request_size_failures() {
        assert!(is_oversize_error(
            "Provider API HTTP 413: request too large"
        ));
        assert!(is_oversize_error("payload too large"));
        assert!(is_oversize_error(
            "Cerebras API Error HTTP 400: Please reduce the length of the messages or completion. Current length is 14859 while limit is 8192"
        ));
        assert!(is_oversize_error(
            "maximum context length exceeded for this request"
        ));
        assert!(is_oversize_error("input too long for the selected model"));
        assert!(!is_oversize_error("HTTP 400 malformed JSON"));
        assert!(!is_oversize_error(
            "Please reduce request concurrency while the service is busy"
        ));
        assert!(!is_oversize_error("HTTP 429 rate limit"));
    }

    #[test]
    fn learned_text_limit_skips_equal_or_larger_inputs_but_allows_smaller_ones() {
        let model = "test-text-size-threshold";
        learn_text_oversize(model, 10_000);
        assert_eq!(rejects_text_size(model, 9_999), None);
        assert_eq!(rejects_text_size(model, 10_000), Some(10_000));
        learn_text_oversize(model, 12_000);
        assert_eq!(rejects_text_size(model, 11_000), Some(10_000));
        learn_text_oversize(model, 8_000);
        assert_eq!(rejects_text_size(model, 8_500), Some(8_000));
    }
}
