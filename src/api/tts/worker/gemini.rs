use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

use super::super::manager::TtsManager;
use super::super::types::{AudioEvent, SOURCE_SAMPLE_RATE, TtsCollectedAudio, TtsRequestProfile};
use super::super::utils::{
    clear_tts_loading_state, clear_tts_state, get_language_instruction_for_code,
    get_language_instruction_for_text,
};
use super::super::websocket::{build_tts_setup, build_tts_text, connect_tts_websocket};
use super::{acquire_warm_socket, start_warm_up};

use crate::APP;
use crate::api::gemini_live::ready_session::{
    ConnectedLiveSocket, LivePoll, LiveSetupServerError, OpenOptions,
};

pub fn synthesize_gemini_live_to_wav_cancel(
    text: &str,
    profile: TtsRequestProfile,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<TtsCollectedAudio> {
    let api_key = {
        let app = APP
            .lock()
            .map_err(|_| anyhow::anyhow!("APP config lock poisoned"))?;
        app.config.gemini_api_key.trim().to_string()
    };
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }

    let model = crate::model_config::normalize_tts_gemini_model(&profile.gemini_model).to_string();
    let instruction = playground_language_instruction(text, &profile);
    let setup = build_tts_setup(
        &model,
        &profile.gemini_voice,
        &profile.gemini_speed,
        instruction.as_deref(),
    );
    let session = ConnectedLiveSocket::connect(&api_key)?.activate_with(
        setup,
        tts_open_options(Duration::from_secs(15)),
        || cancel.load(Ordering::SeqCst),
    );
    if cancel.load(Ordering::SeqCst) {
        return Err(anyhow::anyhow!("Generation cancelled"));
    }
    let mut session = session?;

    if let Err(error) = session.send_json(&build_tts_text(text)) {
        let _ = session.close();
        return Err(error);
    }
    let read_started = Instant::now();
    let mut audio_bytes = Vec::new();
    while !cancel.load(Ordering::SeqCst) {
        if read_started.elapsed() > Duration::from_secs(90) {
            let _ = session.close();
            return Err(anyhow::anyhow!("TTS generation timed out"));
        }
        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                let response_complete = frame.response_complete();
                for audio_data in frame.audio_chunks {
                    audio_bytes.extend_from_slice(&audio_data);
                }
                if response_complete {
                    break;
                }
            }
            Ok(LivePoll::ServerError(error)) => {
                let _ = session.close();
                return Err(anyhow::anyhow!("Gemini TTS error: {error}"));
            }
            Ok(LivePoll::PeerClosed(_)) => break,
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::Idle) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                let _ = session.close();
                return Err(error);
            }
        }
    }
    let _ = session.close();
    if cancel.load(Ordering::SeqCst) {
        return Err(anyhow::anyhow!("Generation cancelled"));
    }
    if audio_bytes.is_empty() {
        return Err(anyhow::anyhow!("TTS generated no audio"));
    }
    let pcm_samples: Vec<i16> = audio_bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let duration_ms = ((pcm_samples.len() as u64) * 1000) / SOURCE_SAMPLE_RATE as u64;
    let wav_data = crate::api::audio::encode_wav(&pcm_samples, SOURCE_SAMPLE_RATE, 1);
    Ok(TtsCollectedAudio {
        pcm_samples,
        wav_data,
        sample_rate: SOURCE_SAMPLE_RATE,
        duration_ms,
    })
}

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
                eprintln!(
                    "[TTS Worker] ERROR: Failed to lock APP for API key: {:?}",
                    e
                );
                let _ = tx.send(AudioEvent::End);
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
        let _ = tx.send(AudioEvent::End);
        clear_tts_loading_state(request.req.hwnd);
        clear_tts_state(request.req.hwnd);
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
                eprintln!("[TTS Worker] ERROR: WebSocket connection failed: {}", e);
                let _ = tx.send(AudioEvent::End);
                clear_tts_loading_state(request.req.hwnd);
                clear_tts_state(request.req.hwnd);
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
            let app = APP.lock().unwrap();
            let voice = app.config.tts_voice.clone();
            let conditions = app.config.tts_language_conditions.clone();

            let instruction = get_language_instruction_for_text(&request.req.text, &conditions);

            if request.req.is_realtime {
                (voice, "Normal".to_string(), instruction)
            } else {
                (voice, app.config.tts_speed.clone(), instruction)
            }
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
    let cancelled = || {
        request.generation < manager.interrupt_generation.load(Ordering::SeqCst)
            || manager.shutdown.load(Ordering::SeqCst)
    };
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

    let mut session = match session {
        Ok(session) => session,
        Err(error) => {
            eprintln!("TTS: Setup failed: {error}");
            let _ = tx.send(AudioEvent::End);
            clear_tts_state(request.req.hwnd);
            return;
        }
    };
    if cancelled() {
        let _ = session.close();
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    // Send request text
    if let Err(e) = session.send_json(&build_tts_text(&request.req.text)) {
        eprintln!("TTS: Failed to send text: {}", e);
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        let _ = session.close();
        return;
    }

    // Read loop
    loop {
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst)
            || manager.shutdown.load(Ordering::SeqCst)
        {
            let _ = session.close();
            let _ = tx.send(AudioEvent::End);
            break;
        }

        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                let response_complete = frame.response_complete();
                for audio_data in frame.audio_chunks {
                    let _ = tx.send(AudioEvent::Data(audio_data));
                }
                if response_complete {
                    let _ = tx.send(AudioEvent::End);
                    break;
                }
            }
            Ok(LivePoll::PeerClosed(_)) => {
                let _ = tx.send(AudioEvent::End);
                break;
            }
            Ok(LivePoll::ServerError(error)) => {
                eprintln!("TTS: Server error: {error}");
                let _ = tx.send(AudioEvent::End);
                clear_tts_state(request.req.hwnd);
                break;
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::Idle) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => {
                eprintln!("TTS: Read error: {}", e);
                let _ = tx.send(AudioEvent::End);
                clear_tts_state(request.req.hwnd);
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
}
