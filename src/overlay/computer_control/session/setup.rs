//! Host-owned validation of the provider's transport setup acknowledgement.

use anyhow::{Result, ensure};
use sgt_computer_control_protocol::MAX_PROVIDER_FRAME_BYTES;

pub(in crate::overlay::computer_control) fn setup_frame_ready(message: &str) -> Result<bool> {
    ensure!(
        message.len() <= MAX_PROVIDER_FRAME_BYTES,
        "Live setup frame exceeds its size limit"
    );
    let frame = crate::api::gemini_live::server_frame::parse_server_frame(message)?;
    if let Some(error) = frame.error {
        let detail = error.chars().take(1024).collect::<String>();
        let root: serde_json::Value = serde_json::from_str(message)?;
        return Err(super::errors::ProviderSetupFailure {
            message: detail,
            resource_limited: root["error"]["code"].as_u64() == Some(429)
                || root["error"]["status"].as_str() == Some("RESOURCE_EXHAUSTED"),
        }
        .into());
    }
    if frame.setup_complete {
        let root: serde_json::Value = serde_json::from_str(message)?;
        ensure!(
            root["setupComplete"].is_object(),
            "Live setup acknowledgement is malformed"
        );
    }
    Ok(frame.setup_complete)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_uses_structural_acknowledgement_and_rejects_wrong_shapes() {
        assert!(setup_frame_ready(r#"{"setupComplete":{}}"#).unwrap());
        assert!(
            !setup_frame_ready(r#"{"note":"setupComplete","nested":{"setupComplete":{}}}"#)
                .unwrap()
        );
        for value in ["null", "false", "true", "[]", r#""ack""#] {
            assert!(setup_frame_ready(&format!(r#"{{"setupComplete":{value}}}"#)).is_err());
        }
        assert!(setup_frame_ready("not json").is_err());
        assert!(setup_frame_ready(&"x".repeat(MAX_PROVIDER_FRAME_BYTES + 1)).is_err());
    }

    #[test]
    fn provider_error_wins_over_a_conflicting_setup_acknowledgement() {
        for error in [
            serde_json::json!({"code": 429, "status": "RESOURCE_EXHAUSTED", "message": "quota exhausted"}),
            serde_json::json!({"code": 400, "status": "INVALID_ARGUMENT", "message": "unsupported setup"}),
        ] {
            let raw = serde_json::json!({"error": error, "setupComplete": {}}).to_string();
            let error = setup_frame_ready(&raw).unwrap_err();
            assert!(error.to_string().contains("Live setup rejected"));
            assert!(super::super::api_key_error_code(&error).is_none());
        }
        let raw = serde_json::json!({"error": {"message": "INVALID_API_KEY:gemini"}}).to_string();
        assert_eq!(
            super::super::api_key_error_code(&setup_frame_ready(&raw).unwrap_err()),
            Some("INVALID_API_KEY:gemini")
        );
    }
}
