//! Provider attempt outcomes for the realtime translation chain: HTTP failures
//! keep their status and Retry-After so the shared model cooldown applies.

use std::io::Read;

use super::ValidatedTranslationResponse;

const MAX_ERROR_BODY_BYTES: u64 = 2048;

/// Why one provider attempt produced no usable translation.
#[derive(Debug)]
pub(super) enum ProviderFailure {
    /// A transport, HTTP or provider error, recorded so the chain cools the
    /// model down (including a reported Retry-After) instead of re-hitting it.
    Request(String),
    /// The provider answered, or could not be asked, without a usable patch.
    Unusable,
}

pub(super) type ProviderResult = Result<ValidatedTranslationResponse, ProviderFailure>;

pub(super) fn unusable<T>(value: Option<T>) -> Result<T, ProviderFailure> {
    value.ok_or(ProviderFailure::Unusable)
}

/// Turns a sent request into its successful response, or a failure message
/// that carries the HTTP status and Retry-After for cooldown classification.
pub(super) fn checked_response(
    provider: &str,
    sent: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<ureq::http::Response<ureq::Body>, ProviderFailure> {
    let response = sent
        .map_err(|error| ProviderFailure::Request(format!("{provider} request failed: {error}")))?;
    let status = response.status().as_u16();
    if response.status().is_success() {
        return Ok(response);
    }
    let retry_after = crate::api::groq::retry_after_seconds(response.headers());
    let mut body = String::new();
    let _ = response
        .into_body()
        .into_reader()
        .take(MAX_ERROR_BODY_BYTES)
        .read_to_string(&mut body);
    Err(ProviderFailure::Request(http_failure_message(
        provider,
        status,
        retry_after,
        &body,
    )))
}

fn http_failure_message(
    provider: &str,
    status: u16,
    retry_after: Option<u64>,
    body: &str,
) -> String {
    let retry_after = retry_after
        .map(|seconds| format!("; retry-after: {seconds}"))
        .unwrap_or_default();
    format!("{provider} HTTP {status}{retry_after}: {}", body.trim())
}

/// Records one chain attempt so failing models cool down and successes close
/// their circuit; returns the usable translation, if any.
pub(super) fn settle_attempt(
    model_id: &str,
    result: ProviderResult,
) -> Option<ValidatedTranslationResponse> {
    match result {
        Ok(response) => {
            crate::retry_model_chain::record_model_success(model_id);
            Some(response)
        }
        Err(ProviderFailure::Request(error)) => {
            crate::retry_model_chain::record_model_failure(model_id, &error);
            None
        }
        Err(ProviderFailure::Unusable) => {
            crate::retry_model_chain::release_model_probe(model_id);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limited_attempt_cools_the_model_down_with_retry_after() {
        let model_id = "realtime-translation-chain-test-429";
        let message = http_failure_message("Groq", 429, Some(30), " rate limited ");
        assert_eq!(message, "Groq HTTP 429; retry-after: 30: rate limited");
        assert!(settle_attempt(model_id, Err(ProviderFailure::Request(message))).is_none());
        let skip = crate::retry_model_chain::claim_model_attempt(model_id)
            .expect("a 429 must open the model's cooldown");
        assert!(skip.contains(model_id), "{skip}");
    }

    #[test]
    fn real_http_failure_keeps_status_and_retry_after_for_the_cooldown() {
        let (endpoint, server) = crate::api::http_test_support::scripted_server(vec![
            "HTTP/1.1 429 Too Many Requests
Retry-After: 30
Content-Length: 12
Connection: close

rate limited",
        ]);
        let sent = crate::api::client::UREQ_RESPONSE_AGENT
            .post(&endpoint)
            .send_json(serde_json::json!({"model": "fixture-model"}));
        let Err(ProviderFailure::Request(message)) = checked_response("Groq", sent) else {
            panic!("a 429 must be a request failure");
        };
        server.join().unwrap();
        assert_eq!(message, "Groq HTTP 429; retry-after: 30: rate limited");
        let model_id = "realtime-translation-chain-test-real-429";
        assert!(settle_attempt(model_id, Err(ProviderFailure::Request(message))).is_none());
        assert!(crate::retry_model_chain::claim_model_attempt(model_id).is_some());
    }

    #[test]
    fn unusable_answer_only_releases_the_probe() {
        let model_id = "realtime-translation-chain-test-unusable";
        assert!(settle_attempt(model_id, Err(ProviderFailure::Unusable)).is_none());
        assert_eq!(
            crate::retry_model_chain::claim_model_attempt(model_id),
            None
        );
    }
}
