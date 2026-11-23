//! One user-visible reporter for audio preset failures (record, stream and
//! file-drop paths), so a failed run never just closes the overlay silently.

use std::sync::Arc;

use crate::overlay::result::ChainCancelToken;

/// Returns the shared key notification code when the failure is a missing or
/// rejected provider key, from either a coded error or a Live setup rejection.
pub(crate) fn audio_failure_key_code(error: &anyhow::Error) -> Option<String> {
    error
        .chain()
        .map(|cause| cause.to_string())
        .find(|message| {
            message.starts_with("NO_API_KEY:") || message.starts_with("INVALID_API_KEY:")
        })
        .or_else(|| {
            crate::api::gemini_live::setup_failure::live_api_key_error_code(error)
                .map(str::to_string)
        })
}

/// Shows the localized key notification for key failures and the error badge
/// for everything else, while the recording that failed is still current.
pub(crate) fn report_audio_failure(token: &Arc<ChainCancelToken>, error: &anyhow::Error) {
    crate::log_info!("[Audio] preset failure: {error:#}");
    if token.is_cancelled() {
        return;
    }
    match audio_failure_key_code(error) {
        Some(code) => {
            let ui_language = crate::APP
                .lock()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_else(|_| "en".to_string());
            crate::overlay::utils::show_api_key_error_notification(&code, &ui_language);
        }
        None => crate::overlay::recording::notify_if_current_recording(
            token,
            &format!("Audio processing failed: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::audio_failure_key_code;

    #[test]
    fn coded_and_live_key_failures_map_to_key_notifications() {
        let missing = anyhow::anyhow!("NO_API_KEY:gemini");
        assert_eq!(
            audio_failure_key_code(&missing).as_deref(),
            Some("NO_API_KEY:gemini")
        );
        let revoked = anyhow::anyhow!(
            "Gemini Live peer closed during setup (code 1007: API key not valid. Please pass a valid API key.)"
        );
        assert_eq!(
            audio_failure_key_code(&revoked).as_deref(),
            Some("INVALID_API_KEY:gemini")
        );
    }

    #[test]
    fn other_failures_use_the_error_badge() {
        let quota = anyhow::anyhow!("Gemini Live error: RESOURCE_EXHAUSTED");
        assert_eq!(audio_failure_key_code(&quota), None);
    }
}
