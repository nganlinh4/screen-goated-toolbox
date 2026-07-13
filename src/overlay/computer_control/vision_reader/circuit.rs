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

#[cfg(test)]
mod tests {
    use super::is_rate_limit_error;

    #[test]
    fn recognizes_structured_and_plain_rate_limits() {
        assert!(is_rate_limit_error(
            "Groq vision API HTTP 429: TPM exhausted"
        ));
        assert!(is_rate_limit_error("quota exceeded for this model"));
        assert!(!is_rate_limit_error("HTTP 503 temporarily unavailable"));
    }
}
