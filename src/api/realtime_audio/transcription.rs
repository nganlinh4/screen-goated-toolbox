//! Main transcription loop for realtime audio

mod main_loop;

use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;

use crate::APP;
use crate::config::Preset;
use crate::model_config::{
    normalize_realtime_transcription_model_id, realtime_transcription_api_model,
};
use crate::overlay::realtime_webview::SELECTED_APP_PID;

use super::REALTIME_RMS;
use super::capture::{
    start_device_loopback_capture_resilient, start_mic_capture_resilient, start_per_app_capture,
};
use super::state::SharedRealtimeState;
use super::translation::run_translation_loop;
use super::websocket::{
    connect_websocket, is_transient_socket_read_error, send_setup_message, set_socket_nonblocking,
    set_socket_short_timeout,
};

fn wait_for_selected_audio_app(stop_signal: &Arc<AtomicBool>) -> Option<u32> {
    let started = Instant::now();
    while !stop_signal.load(Ordering::SeqCst) {
        let pid = SELECTED_APP_PID.load(Ordering::SeqCst);
        if pid > 0 {
            return Some(pid);
        }
        if crate::overlay::realtime_webview::AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
            || crate::overlay::realtime_webview::TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
            || started.elapsed() > Duration::from_secs(30)
        {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// Start realtime audio transcription
pub fn start_realtime_transcription(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
) {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let overlay_send = crate::win_types::SendHwnd(overlay_hwnd);
    let translation_send = translation_hwnd.map(crate::win_types::SendHwnd);

    std::thread::spawn(move || {
        transcription_thread_entry(
            preset,
            stop_signal,
            overlay_send,
            translation_send,
            state,
            session_id,
        );
    });
}

fn should_run_text_translation_loop(
    preset: &Preset,
    translation_hwnd: Option<HWND>,
    trans_model: &str,
) -> bool {
    translation_hwnd.is_some()
        && preset.blocks.len() > 1
        && !crate::model_config::is_gemini_live_s2s_model_id(trans_model)
}

fn spawn_text_translation_loop(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    translation_send: crate::win_types::SendHwnd,
    state: SharedRealtimeState,
    trans_model: &str,
) {
    crate::log_info!(
        "[RealtimeTranslate] spawn text translation loop transcription_model={}",
        trans_model
    );
    std::thread::spawn(move || {
        run_translation_loop(preset, stop_signal, translation_send, state);
    });
}

fn transcription_thread_entry(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_send: crate::win_types::SendHwnd,
    translation_send: Option<crate::win_types::SendHwnd>,
    state: SharedRealtimeState,
    session_id: u64,
) {
    let hwnd_overlay = overlay_send.0;
    let hwnd_translation = translation_send.map(|h| h.0);

    use crate::overlay::realtime_webview::{
        AUDIO_SOURCE_CHANGE, LANGUAGE_CHANGE, NEW_AUDIO_SOURCE, NEW_TRANSCRIPTION_MODEL,
        TRANSCRIPTION_MODEL_CHANGE,
    };

    let mut current_preset = preset;
    let mut freeze_on_next_restart = false;
    let mut text_translation_loop_active = false;

    loop {
        if is_stale_session(session_id) {
            break;
        }

        AUDIO_SOURCE_CHANGE.store(false, Ordering::SeqCst);
        TRANSCRIPTION_MODEL_CHANGE.store(false, Ordering::SeqCst);
        LANGUAGE_CHANGE.store(false, Ordering::SeqCst);
        super::DEVICE_RECONNECT_REQUESTED.store(false, Ordering::SeqCst);

        // On model/source switch: freeze old transcript so it stays visible,
        // then let the new session start fresh.
        if freeze_on_next_restart {
            if let Ok(mut s) = state.lock() {
                s.freeze_current_transcript();
            }
            freeze_on_next_restart = false;
        }

        // Reset volume indicator to ensure fresh state when switching methods
        REALTIME_RMS.store(0, Ordering::SeqCst);

        let trans_model = {
            let app = APP.lock().unwrap();
            normalize_realtime_transcription_model_id(&app.config.realtime_transcription_model)
        };

        let wants_text_translation =
            should_run_text_translation_loop(&current_preset, hwnd_translation, &trans_model);
        if wants_text_translation && !text_translation_loop_active {
            if let Some(t_send) = translation_send {
                spawn_text_translation_loop(
                    current_preset.clone(),
                    stop_signal.clone(),
                    t_send,
                    state.clone(),
                    &trans_model,
                );
                text_translation_loop_active = true;
            }
        } else if !wants_text_translation {
            if crate::model_config::is_gemini_live_s2s_model_id(&trans_model)
                && hwnd_translation.is_some()
            {
                crate::log_info!(
                    "[RealtimeTranslate] skip text translation loop because direct speech model owns target output transcription_model={}",
                    trans_model
                );
            }
            text_translation_loop_active = false;
        }

        // Update state with selected method immediately (before potentially slow model loading)
        if let Ok(mut s) = state.lock() {
            if trans_model == "parakeet" {
                s.set_transcription_method(super::state::TranscriptionMethod::Parakeet);
            } else if trans_model == crate::model_config::QWEN3_ASR_0_6B_MODEL_ID
                || trans_model == crate::model_config::QWEN3_ASR_1_7B_MODEL_ID
            {
                s.set_transcription_method(super::state::TranscriptionMethod::Qwen3Local);
            } else if trans_model == "zipformer" {
                s.set_transcription_method(super::state::TranscriptionMethod::SherpaZipformer);
            } else if crate::model_config::is_gemini_live_s2s_model_id(&trans_model) {
                s.set_transcription_method(super::state::TranscriptionMethod::GeminiLiveS2s);
            } else {
                s.set_transcription_method(super::state::TranscriptionMethod::GeminiLive);
            }
        }

        let result = if trans_model == "parakeet" {
            let dummy_pause = Arc::new(AtomicBool::new(false));
            super::parakeet::run_parakeet_transcription(
                current_preset.clone(),
                stop_signal.clone(),
                dummy_pause,
                None,
                false,
                Some(hwnd_overlay),
                state.clone(),
            )
        } else if trans_model == crate::model_config::QWEN3_ASR_0_6B_MODEL_ID {
            super::qwen3::run_qwen3_transcription_variant(
                current_preset.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                state.clone(),
                super::qwen3::Qwen3ModelVariant::Small,
            )
        } else if trans_model == crate::model_config::QWEN3_ASR_1_7B_MODEL_ID {
            super::qwen3::run_qwen3_transcription_variant(
                current_preset.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                state.clone(),
                super::qwen3::Qwen3ModelVariant::Large,
            )
        } else if trans_model == "zipformer" {
            super::sherpa_onnx::run_sherpa_transcription(
                current_preset.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                state.clone(),
                session_id,
            )
        } else if crate::model_config::is_gemini_live_s2s_model_id(&trans_model) {
            super::s2s::run_gemini_live_s2s(
                current_preset.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                hwnd_translation,
                state.clone(),
                session_id,
            )
        } else {
            run_realtime_transcription(
                current_preset.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                hwnd_translation,
                state.clone(),
            )
        };

        let reconnect_requested = super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst);
        if is_stale_session(session_id) {
            break;
        }

        if let Err(e) = result {
            // Only show error if it's not a user-initiated action (model/source change, stop signal)
            let is_user_initiated = AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
                || reconnect_requested
                || super::websocket::is_recoverable_anyhow_socket_error(&e)
                || stop_signal.load(Ordering::Relaxed);

            if !is_user_initiated {
                let err_msg = format!(" [Error: {}]", e);
                eprintln!("Realtime transcription error: {}", e);

                // Append error to state so it's visible in the window
                if let Ok(mut s) = state.lock() {
                    s.append_transcript(&err_msg);
                }

                // Force immediate UI update
                let display_text = if let Ok(s) = state.lock() {
                    s.display_transcript.clone()
                } else {
                    String::new()
                };
                use super::utils::update_overlay_text;
                update_overlay_text(hwnd_overlay, &display_text);

                // Do NOT close the window - let the user see the error
            }
        }

        let restart_source = AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst);
        let restart_model = TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst);
        let restart_language = LANGUAGE_CHANGE.load(Ordering::SeqCst);

        if restart_source
            && let Ok(new_source) = NEW_AUDIO_SOURCE.lock()
            && !new_source.is_empty()
        {
            // println!("Changing audio source to: {}", new_source);
            let mut app = APP.lock().unwrap();
            app.config.realtime_audio_source = new_source.clone();
            current_preset.audio_source = new_source.clone();
            // Save config? Optional, but UI should sync.
        }

        if restart_model
            && let Ok(new_model) = NEW_TRANSCRIPTION_MODEL.lock()
            && !new_model.is_empty()
        {
            // println!("Changing transcription model to: {}", new_model);
            let mut app = APP.lock().unwrap();
            app.config.realtime_transcription_model =
                normalize_realtime_transcription_model_id(&new_model);
        }

        // If a restart is triggered, reset stop signal to allow the new transcription to run
        if restart_source || restart_model || restart_language {
            freeze_on_next_restart = true;
            stop_signal.store(false, Ordering::SeqCst);
        }

        if reconnect_requested {
            stop_signal.store(false, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(750));
            continue;
        }

        if !restart_source
            && !restart_model
            && !restart_language
            && stop_signal.load(Ordering::Relaxed)
        {
            break;
        }
        // If a restart is triggered (source or model changed), the loop continues.
        // Otherwise, if stop_signal is set, we break.
        // If neither, we also break, meaning the transcription loop only runs once
        // unless a restart is explicitly requested.
        if !restart_source && !restart_model && !restart_language {
            break;
        }
    }

    crate::overlay::realtime_webview::state::REALTIME_SESSION_STOPPING
        .store(false, Ordering::SeqCst);
}

fn is_stale_session(session_id: u64) -> bool {
    crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst)
        != session_id
}

fn run_realtime_transcription(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    _translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
) -> Result<()> {
    let (gemini_api_key, selected_model_id) = {
        let app = APP.lock().unwrap();
        (
            app.config.gemini_api_key.clone(),
            normalize_realtime_transcription_model_id(&app.config.realtime_transcription_model),
        )
    };
    let gemini_live_model = realtime_transcription_api_model(&selected_model_id);

    if gemini_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }

    // println!("Gemini: Connecting to WebSocket...");
    let mut socket = connect_websocket(&gemini_api_key)?;
    // println!("Gemini: Connected! Sending setup...");
    send_setup_message(&mut socket, &gemini_live_model)?;
    // println!("Gemini: Setup sent, waiting for acknowledgment...");

    // Set transcription method to GeminiLive (uses delimiter-based segmentation)
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::GeminiLive);
    }

    // Set short timeout so we can check for model changes during setup
    set_socket_short_timeout(&mut socket)?;

    // Wait for setup acknowledgment
    let setup_start = Instant::now();
    loop {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let msg = msg.as_str();
                if msg.contains("setupComplete") {
                    break;
                }
                if let Some(err) = crate::api::gemini_live::websocket::parse_error(msg) {
                    return Err(anyhow::anyhow!("Server returned error: {}", err));
                }
            }
            Ok(tungstenite::Message::Close(frame)) => {
                let close_info = frame
                    .map(|f| format!("code={}, reason={}", f.code, f.reason))
                    .unwrap_or("no frame".to_string());
                return Err(anyhow::anyhow!(
                    "Connection closed by server: {}",
                    close_info
                ));
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    if text.contains("setupComplete") {
                        break;
                    }
                } else if data.len() < 100 {
                    break;
                }
            }
            Ok(_) => {}
            Err(e) if is_transient_socket_read_error(&e) => {
                if setup_start.elapsed() > Duration::from_secs(30) {
                    return Err(anyhow::anyhow!("Setup timeout - no response from server"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(e.into());
            }
        }
        // Check for stop signal
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
        // Check for model change or audio source change signals
        use crate::overlay::realtime_webview::{AUDIO_SOURCE_CHANGE, TRANSCRIPTION_MODEL_CHANGE};
        if TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
            || AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
        {
            // println!("Gemini: Model/source change detected during setup, aborting...");
            return Ok(()); // Return cleanly to allow the outer loop to handle the change
        }
    }

    set_socket_nonblocking(&mut socket)?;

    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));

    use crate::overlay::realtime_webview::REALTIME_TTS_ENABLED;
    let tts_enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let selected_pid = if preset.audio_source == "device" && tts_enabled && selected_pid == 0 {
        crate::log_info!(
            "[RealtimeGeminiLiveHealth] app-selection-required source=device tts_enabled=true"
        );
        crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
        wait_for_selected_audio_app(&stop_signal)
    } else if selected_pid > 0 {
        Some(selected_pid)
    } else {
        None
    };

    let using_per_app_capture =
        preset.audio_source == "device" && tts_enabled && selected_pid.is_some();
    let using_device_loopback = preset.audio_source == "device" && !tts_enabled;

    let _stream: Option<cpal::Stream>;

    let dummy_pause = Arc::new(AtomicBool::new(false));

    if using_per_app_capture {
        #[cfg(target_os = "windows")]
        {
            let selected_pid = selected_pid.unwrap_or_default();
            start_per_app_capture(
                selected_pid,
                audio_buffer.clone(),
                stop_signal.clone(),
                dummy_pause.clone(),
            )?;
        }
        _stream = None;
    } else if using_device_loopback {
        _stream = Some(start_device_loopback_capture_resilient(
            audio_buffer.clone(),
            stop_signal.clone(),
            dummy_pause.clone(),
        )?);
    } else if preset.audio_source == "device" && tts_enabled {
        crate::log_info!(
            "[RealtimeGeminiLiveHealth] no-capture reason=app-selection-cancelled source=device tts_enabled=true"
        );
        return Ok(());
    } else {
        _stream = Some(start_mic_capture_resilient(
            audio_buffer.clone(),
            stop_signal.clone(),
            dummy_pause.clone(),
        )?);
    }

    // Start translation thread if needed
    // NOTE: Translation thread is now spawned in `start_realtime_transcription`
    // to ensure it runs independent of the transcription model (Parakeet/Gemini).

    // Main loop
    let capture_label = if using_per_app_capture {
        "per-app"
    } else if using_device_loopback {
        "device"
    } else {
        "mic"
    };
    let loop_result = main_loop::run_main_loop(main_loop::RealtimeMainLoop {
        socket,
        audio_buffer,
        stop_signal,
        overlay_hwnd,
        state,
        gemini_live_model: &gemini_live_model,
        gemini_api_key: &gemini_api_key,
        capture_label,
    });

    drop(_stream);
    loop_result
}
