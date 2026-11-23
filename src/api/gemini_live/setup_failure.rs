//! Structural classification of Gemini Live setup failures, shared by every
//! Live client (TTS, preset dictation, realtime transcription).
//!
//! Live reports a rejected key as a setup close reason or setup error message
//! rather than an HTTP status, so callers map it here to the shared
//! `NO_API_KEY:gemini` / `INVALID_API_KEY:gemini` codes that drive the
//! localized key notification. Network, quota and model failures stay
//! unclassified so they are never mislabeled as a bad key.

#[cfg(not(feature = "recorder-worker"))]
use super::ready_session::LiveSetupServerError;

/// Reconnect budget shared by Live transcription streams (host only).
#[cfg(not(feature = "recorder-worker"))]
pub const LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS: u32 = 6;

pub const NO_API_KEY: &str = "NO_API_KEY:gemini";
pub const INVALID_API_KEY: &str = "INVALID_API_KEY:gemini";

/// Returns the shared key error code when any cause in the chain is an
/// explicit Gemini credential rejection.
pub fn live_api_key_error_code(error: &anyhow::Error) -> Option<&'static str> {
    error
        .chain()
        .find_map(|cause| api_key_error_code_text(&cause.to_string()))
}

/// A setup failure that a reconnect cannot fix: a non-retryable server setup
/// error, an explicit key rejection, or a client-side HTTP upgrade rejection.
/// Only host reconnect loops use it; the recorder worker has none.
#[cfg(not(feature = "recorder-worker"))]
pub fn fatal_setup_error(error: &anyhow::Error) -> bool {
    if let Some(server) = error.downcast_ref::<LiveSetupServerError>() {
        return !server.server.retryable;
    }
    if let Some(tungstenite::Error::Http(response)) = error.downcast_ref::<tungstenite::Error>() {
        let code = response.status().as_u16();
        return (400..500).contains(&code) && code != 408 && code != 429;
    }
    live_api_key_error_code(error).is_some()
}

fn api_key_error_code_text(message: &str) -> Option<&'static str> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("no_api_key") {
        return Some(NO_API_KEY);
    }
    let rejected = [
        "invalid_api_key",
        "api_key_invalid",
        "api_key_expired",
        "api key not valid",
        "api key is not valid",
        "api key expired",
        "invalid api key",
        "api key is invalid",
    ];
    rejected
        .iter()
        .any(|marker| lower.contains(marker))
        .then_some(INVALID_API_KEY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_close_reason_with_rejected_key_maps_to_invalid_key() {
        let error = anyhow::anyhow!(
            "Gemini Live peer closed during setup (code 1007: API key not valid. Please pass a valid API key.)"
        )
        .context("TTS setup");
        assert_eq!(live_api_key_error_code(&error), Some(INVALID_API_KEY));
        let expired = anyhow::anyhow!("Gemini Live setup error: API_KEY_EXPIRED");
        assert_eq!(live_api_key_error_code(&expired), Some(INVALID_API_KEY));
    }

    #[cfg(not(feature = "recorder-worker"))]
    #[test]
    fn key_rejection_is_fatal_but_transport_loss_is_not() {
        let rejected = anyhow::anyhow!(
            "Gemini Live peer closed during setup (code 1008: API key expired. Please renew the API key.)"
        );
        assert!(fatal_setup_error(&rejected));
        assert!(!fatal_setup_error(&anyhow::anyhow!(
            "read Gemini Live setup response: connection reset"
        )));
    }

    #[test]
    fn quota_and_network_failures_are_not_key_errors() {
        for message in [
            "Gemini Live setup error: RESOURCE_EXHAUSTED quota exceeded",
            "Gemini Live setup timed out",
            "read Gemini Live setup response: connection reset",
        ] {
            assert_eq!(live_api_key_error_code(&anyhow::anyhow!(message)), None);
        }
    }
}
