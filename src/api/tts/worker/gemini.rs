use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

use super::super::manager::TtsManager;
use super::super::types::{AudioEvent, TtsRequestProfile};
use super::super::utils::{get_language_instruction_for_code, get_language_instruction_for_text};
use super::super::websocket::{build_tts_setup, build_tts_text, connect_tts_websocket};
use super::open_weights::fail_request;
use super::{acquire_warm_socket, start_warm_up};

use crate::APP;
use crate::api::gemini_live::ready_session::{
    ConnectedLiveSocket, LiveCloseInfo, LivePoll, LiveSetupServerError, OpenOptions,
};
use crate::api::gemini_live::setup_failure::live_api_key_error_code;

#[cfg(feature = "recorder-worker")]
mod narration_wav;
#[cfg(feature = "recorder-worker")]
pub use narration_wav::synthesize_gemini_live_to_wav_cancel;

const PROVIDER: &str = "Gemini";

pub(super) fn handle_gemini_tts(
    manager: &Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    // Get API key
    let api_key = {
        match APP.lock() {
            Ok(app) => app.config.gemini_api_key.clone(),
            Err(e) => {
                fail_request(
                    PROVIDER,
                    request.req.hwnd,
                    &tx,
                    format!("Failed to lock APP for API key: {e:?}"),
                );
                std::thread::sleep(Duration::from_secs(1));
                return;
            }
        }
    };

    if api_key.trim().is_empty() {
        eprintln!("[TTS Worker] ERROR: No Gemini API key configured - TTS will not work");
        let lang = match APP.lock() {
            Ok(app) => app.config.ui_language.clone(),
            Err(_) => "en".to_string(),
        };
        crate::overlay::utils::show_api_key_error_notification("NO_API_KEY:gemini", &lang);
        fail_request(PROVIDER, request.req.hwnd, &tx, "NO_API_KEY:gemini");
        std::thread::sleep(Duration::from_secs(5));
        return;
    }

    // Try warm socket first (saves ~800ms)
    let (socket, used_warm_socket) = if let Some(warm) = acquire_warm_socket(&api_key) {
        (warm, true)
    } else {
        eprintln!("[TTS Worker] Connecting to Gemini WebSocket...");
        let socket_result = connect_tts_websocket(&api_key);
        match socket_result {
            Ok(s) => {
                eprintln!("[TTS Worker] WebSocket connected successfully");
                (s, false)
            }
            Err(e) => {
                fail_request(
                    PROVIDER,
                    request.req.hwnd,
                    &tx,
                    format!("Gemini TTS connection failed: {e}"),
                );
                std::thread::sleep(Duration::from_secs(3));
                return;
            }
        }
    };

    let (current_voice, current_speed, language_instruction) =
        if let Some(profile) = request.req.profile.as_ref() {
            (
                profile.gemini_voice.clone(),
                profile.gemini_speed.clone(),
                playground_language_instruction(&request.req.text, profile),
            )
        } else {
            app_voice_settings(
                &request.req.text,
                request.req.is_realtime,
                get_language_instruction_for_text,
            )
        };
    let current_model = if let Some(profile) = request.req.profile.as_ref() {
        crate::model_config::normalize_tts_gemini_model(&profile.gemini_model).to_string()
    } else {
        let app = APP.lock().unwrap();
        crate::model_config::normalize_tts_gemini_model(&app.config.tts_gemini_live_model)
            .to_string()
    };

    let setup = build_tts_setup(
        &current_model,
        &current_voice,
        &current_speed,
        language_instruction.as_deref(),
    );
    let cancelled = || request.is_cancelled(manager);
    let activate = |socket| {
        ConnectedLiveSocket::from_socket(socket).activate_with(
            setup.clone(),
            tts_open_options(Duration::from_secs(10)),
            cancelled,
        )
    };
    let first_activation = activate(socket);
    let session = if first_activation
        .as_ref()
        .is_err_and(retryable_before_content)
        && used_warm_socket
        && !cancelled()
    {
        eprintln!("[TTS Worker] Warm socket was stale; retrying a fresh connection");
        connect_tts_websocket(&api_key).and_then(activate)
    } else {
        first_activation
    };

    if manager.shutdown.load(Ordering::SeqCst) {
        return;
    }

    let mut session = match session.map_err(classify_setup_error) {
        Ok(session) => session,
        Err(error) => {
            let message = error.to_string();
            if live_api_key_error_code(&error).is_some() {
                let lang = APP
                    .lock()
                    .map(|app| app.config.ui_language.clone())
                    .unwrap_or_else(|_| "en".to_string());
                crate::overlay::utils::show_api_key_error_notification(&message, &lang);
            }
            fail_request(PROVIDER, request.req.hwnd, &tx, message);
            return;
        }
    };
    if cancelled() {
        let _ = session.close();
        let _ = tx.send(AudioEvent::End);
        return;
    }

    // Send request text
    if let Err(e) = session.send_json(&build_tts_text(&request.req.text)) {
        fail_request(
            PROVIDER,
            request.req.hwnd,
            &tx,
            format!("Gemini TTS failed to send text: {e}"),
        );
        let _ = session.close();
        return;
    }

    // Read loop
    loop {
        if cancelled() {
            let _ = session.close();
            let _ = tx.send(AudioEvent::End);
            break;
        }

        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                let response_complete = frame.response_complete();
                for audio_data in frame.audio_chunks {
                    if tx.send(AudioEvent::Data(audio_data)).is_err() {
                        let _ = session.close();
                        return;
                    }
                }
                if response_complete {
                    let _ = tx.send(AudioEvent::End);
                    break;
                }
            }
            Ok(LivePoll::PeerClosed(close)) => {
                fail_request(
                    PROVIDER,
                    request.req.hwnd,
                    &tx,
                    premature_close_message(close.as_ref()),
                );
                break;
            }
            Ok(LivePoll::ServerError(error)) => {
                fail_request(
                    PROVIDER,
                    request.req.hwnd,
                    &tx,
                    format!("Gemini TTS error: {error}"),
                );
                break;
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::Idle) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => {
                fail_request(
                    PROVIDER,
                    request.req.hwnd,
                    &tx,
                    format!("Gemini TTS read error: {e}"),
                );
                break;
            }
        }
    }

    let _ = session.close();

    // Do not create a transport the manager will immediately discard at shutdown.
    if !manager.shutdown.load(Ordering::SeqCst) {
        start_warm_up(api_key);
    }
}

fn tts_open_options(setup_timeout: Duration) -> OpenOptions {
    OpenOptions {
        setup_timeout,
        setup_read_timeout: Duration::from_millis(200),
        active_read_timeout: Duration::from_millis(50),
    }
}

/// Maps an explicit credential rejection during Live setup to the shared
/// `INVALID_API_KEY:gemini` code; other setup failures keep their detail.
fn classify_setup_error(error: anyhow::Error) -> anyhow::Error {
    match live_api_key_error_code(&error) {
        Some(code) => anyhow::anyhow!(code),
        None => error,
    }
}

/// A close before `turnComplete` truncates the clip, so it is a failure.
fn premature_close_message(close: Option<&LiveCloseInfo>) -> String {
    let detail = close.map_or_else(
        || "no close details".to_string(),
        |close| format!("code {}: {}", close.code, close.reason),
    );
    format!("Gemini TTS connection closed before turn completion ({detail})")
}

/// Voice, speed and language instruction from the app settings. The
/// instruction may wait on remote language detection, so the settings are
/// copied first and the lock is released before it runs.
fn app_voice_settings(
    text: &str,
    is_realtime: bool,
    language_instruction: impl FnOnce(&str, &[crate::config::TtsLanguageCondition]) -> Option<String>,
) -> (String, String, Option<String>) {
    let (voice, speed, conditions) = {
        let app = APP.lock().unwrap();
        (
            app.config.tts_voice.clone(),
            app.config.tts_speed.clone(),
            app.config.tts_language_conditions.clone(),
        )
    };
    let instruction = language_instruction(text, &conditions);
    let speed = if is_realtime {
        "Normal".to_string()
    } else {
        speed
    };
    (voice, speed, instruction)
}

fn retryable_before_content(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<LiveSetupServerError>()
        .is_none_or(|setup| setup.server.retryable)
}

fn playground_language_instruction(text: &str, profile: &TtsRequestProfile) -> Option<String> {
    let language_instruction = profile
        .language_code_override
        .as_deref()
        .and_then(|code| {
            get_language_instruction_for_code(code, &profile.gemini_language_conditions)
        })
        .or_else(|| get_language_instruction_for_text(text, &profile.gemini_language_conditions));
    let custom_instruction = profile.gemini_instruction.trim();
    match (language_instruction, custom_instruction.is_empty()) {
        (Some(language_instruction), false) => {
            Some(format!("{language_instruction} {custom_instruction}"))
        }
        (Some(language_instruction), true) => Some(language_instruction),
        (None, false) => Some(custom_instruction.to_string()),
        (None, true) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::gemini_live::ready_session::LiveServerError;

    #[test]
    fn warm_retry_rejects_fatal_server_setup_errors() {
        let fatal = anyhow::Error::new(LiveSetupServerError {
            server: LiveServerError {
                message: "invalid".to_string(),
                retryable: false,
            },
        });
        let transient = anyhow::Error::new(LiveSetupServerError {
            server: LiveServerError {
                message: "unavailable".to_string(),
                retryable: true,
            },
        });

        assert!(!retryable_before_content(&fatal));
        assert!(retryable_before_content(&transient));
        assert!(retryable_before_content(&anyhow::anyhow!("stale socket")));
    }

    #[test]
    fn setup_key_rejection_becomes_the_shared_invalid_key_code() {
        let rejected = anyhow::anyhow!(
            "Gemini Live peer closed during setup (code 1007: API key not valid. Please pass a valid API key.)"
        );
        assert_eq!(
            classify_setup_error(rejected).to_string(),
            "INVALID_API_KEY:gemini"
        );
        let quota = anyhow::anyhow!("Gemini Live setup error: RESOURCE_EXHAUSTED");
        assert!(
            classify_setup_error(quota)
                .to_string()
                .contains("RESOURCE_EXHAUSTED")
        );
    }

    #[test]
    fn premature_close_names_the_close_detail() {
        let close = LiveCloseInfo {
            code: 1011,
            reason: "internal".to_string(),
        };
        assert_eq!(
            premature_close_message(Some(&close)),
            "Gemini TTS connection closed before turn completion (code 1011: internal)"
        );
    }

    #[test]
    fn app_settings_are_not_locked_while_the_language_is_detected() {
        let mut detected = false;
        let (_, speed, instruction) = app_voice_settings("xin chào", true, |text, _| {
            assert!(
                super::super::worker_edge::settings_tests::app_lock_is_free(),
                "the app settings lock is held during detection"
            );
            detected = text == "xin chào";
            Some("Speak Vietnamese.".to_string())
        });
        assert!(detected);
        assert_eq!(speed, "Normal");
        assert_eq!(instruction.as_deref(), Some("Speak Vietnamese."));
    }
}
