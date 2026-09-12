use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(crate) fn retry_after_seconds(headers: &ureq::http::HeaderMap) -> Option<u64> {
    let seconds = headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .parse::<f64>()
        .ok()?;
    (seconds.is_finite() && seconds >= 0.0).then(|| seconds.ceil() as u64)
}

pub(crate) fn groq_rate_limit_retry_delay(
    status: u16,
    rate_attempt: u8,
    retry_after: Option<u64>,
) -> Option<u64> {
    (status == 429 && rate_attempt == 0)
        .then_some(retry_after)
        .flatten()
        .filter(|seconds| *seconds <= 2)
}

pub(crate) fn wait_for_groq_retry(seconds: u64, cancel_token: &Option<Arc<AtomicBool>>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if cancel_token
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Relaxed))
        {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_short_structural_rate_limit_delay_is_retried() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/text-provider-routing.json"
        ))
        .unwrap();
        let retry = &fixture["structured_text_contract"]["locally_validated_stream"]["groq_short_rate_limit_retry"];
        assert_eq!(
            groq_rate_limit_retry_delay(429, 0, retry["max_delay_seconds"].as_u64()),
            Some(2)
        );
        assert_eq!(
            groq_rate_limit_retry_delay(429, retry["attempts"].as_u64().unwrap() as u8, Some(1)),
            None
        );
        assert_eq!(groq_rate_limit_retry_delay(429, 0, Some(3)), None);
        assert_eq!(groq_rate_limit_retry_delay(429, 0, None), None);
        assert_eq!(groq_rate_limit_retry_delay(503, 0, Some(1)), None);
        let mut headers = ureq::http::HeaderMap::new();
        for (value, expected) in [
            ("0.42", Some(1)),
            ("2.01", Some(3)),
            ("-1", None),
            ("NaN", None),
            ("unknown", None),
        ] {
            headers.insert("retry-after", value.parse().unwrap());
            assert_eq!(retry_after_seconds(&headers), expected);
        }
        assert!(!wait_for_groq_retry(
            0,
            &Some(Arc::new(AtomicBool::new(true)))
        ));
    }
}
