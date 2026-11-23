//! One mapping from a failed Groq HTTP response to the error text the retry
//! chain classifies.

/// 401/403 become the invalid-key signal; anything else keeps the
/// output-limit prefix the chain retries on and a bounded provider reason.
pub(crate) fn http_error(status: u16, body: &str) -> anyhow::Error {
    if matches!(status, 401 | 403) {
        return anyhow::anyhow!("INVALID_API_KEY:groq");
    }
    anyhow::anyhow!(
        "{}Groq API HTTP {status}: {}",
        super::output_limit::failure_prefix(status, body),
        provider_reason(status, body)
    )
}

fn provider_reason(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|root| {
            root.pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| format!("request failed with status {status}"));
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_error_keeps_bounded_provider_reason() {
        let body = serde_json::json!({
            "error": {"message": format!("payload too large: {}", "x".repeat(800))}
        })
        .to_string();
        let message = provider_reason(413, &body);
        assert!(message.starts_with("payload too large:"));
        assert_eq!(message.chars().count(), 500);
    }

    #[test]
    fn malformed_error_body_reports_only_the_status() {
        assert_eq!(
            provider_reason(429, "not-json"),
            "request failed with status 429"
        );
    }

    #[test]
    fn rejected_key_is_the_invalid_key_signal() {
        for status in [401, 403] {
            assert_eq!(http_error(status, "{}").to_string(), "INVALID_API_KEY:groq");
        }
        assert!(
            http_error(500, "not-json")
                .to_string()
                .contains("Groq API HTTP 500: request failed with status 500")
        );
    }
}
