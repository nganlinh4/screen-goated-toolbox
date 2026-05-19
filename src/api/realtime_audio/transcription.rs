//! Main transcription loop for realtime audio

use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::APP;
use crate::config::Preset;
use crate::model_config::{
    normalize_realtime_transcription_model_id, realtime_transcription_api_model,
};
use crate::overlay::realtime_webview::SELECTED_APP_PID;

use super::capture::{start_device_loopback_capture, start_mic_capture, start_per_app_capture};
use super::state::SharedRealtimeState;
use super::translation::run_translation_loop;
use super::utils::update_overlay_text;
use super::websocket::{
    connect_websocket, parse_input_transcription, send_audio_chunk, send_setup_message,
    set_socket_nonblocking, set_socket_short_timeout,
};
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};

/// Audio mode state machine for silence injection
#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

impl AudioMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Silence => "silence",
            Self::CatchUp => "catchup",
        }
    }
}

fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1_000) / 16_000
}

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

    let trans_model = APP
        .lock()
        .map(|app| {
            normalize_realtime_transcription_model_id(&app.config.realtime_transcription_model)
        })
        .unwrap_or_else(|_| crate::model_config::GEMINI_LIVE_AUDIO_MODEL_ID_2_5.to_string());
    let is_s2s = trans_model == "gemini-live-s2s";

    // Spawn translation thread if needed. Direct S2S owns translation audio itself.
    let has_translation = translation_hwnd.is_some() && preset.blocks.len() > 1 && !is_s2s;
    if has_translation {
        crate::log_info!(
            "[RealtimeTranslate] spawn text translation loop transcription_model={}",
            trans_model
        );
        let t_send = translation_send.unwrap();
        let t_state = state.clone();
        let t_stop = stop_signal.clone();
        let t_preset = preset.clone();

        std::thread::spawn(move || {
            run_translation_loop(t_preset, t_stop, t_send, t_state);
        });
    } else if translation_hwnd.is_some() && is_s2s {
        crate::log_info!(
            "[RealtimeTranslate] skip text translation loop because Gemini S2S owns target output"
        );
    }

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
            } else if trans_model == "gemini-live-s2s" {
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
        } else if trans_model == "gemini-live-s2s" {
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
                if msg.contains("error") || msg.contains("Error") {
                    return Err(anyhow::anyhow!("Server returned error: {}", msg));
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
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
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
        _stream = Some(start_device_loopback_capture(
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
        _stream = Some(start_mic_capture(
            audio_buffer.clone(),
            stop_signal.clone(),
            dummy_pause.clone(),
        )?);
    }

    // Start translation thread if needed
    // NOTE: Translation thread is now spawned in `start_realtime_transcription`
    // to ensure it runs independent of the transcription model (Parakeet/Gemini).

    // Main loop
    run_main_loop(
        socket,
        audio_buffer,
        stop_signal,
        overlay_hwnd,
        state,
        &gemini_live_model,
        &gemini_api_key,
    )?;

    drop(_stream);
    Ok(())
}

fn run_main_loop(
    mut socket: tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
    gemini_live_model: &str,
    gemini_api_key: &str,
) -> Result<()> {
    let mut last_send = Instant::now();
    let send_interval = Duration::from_millis(100);

    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = Instant::now();
    let mut silence_buffer: Vec<i16> = Vec::new();

    const NORMAL_DURATION: Duration = Duration::from_secs(20);
    const SILENCE_DURATION: Duration = Duration::from_secs(2);
    const SAMPLES_PER_100MS: usize = 1600;

    let mut last_transcription_time = Instant::now();
    let mut consecutive_empty_reads: u32 = 0;
    const NO_RESULT_THRESHOLD_SECS: u64 = 8;
    const EMPTY_READ_CHECK_COUNT: u32 = 50;

    let session_started = Instant::now();
    let mut last_health_log = Instant::now();
    let mut sent_chunks = 0u64;
    let mut sent_samples = 0usize;
    let mut transcript_updates = 0u64;
    let mut transcript_chars = 0usize;
    let mut reconnect_count = 0u32;

    while !stop_signal.load(Ordering::Relaxed) {
        if overlay_hwnd.0 != 0 as _ && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            stop_signal.store(true, Ordering::SeqCst);
            break;
        }

        {
            use crate::overlay::realtime_webview::{
                AUDIO_SOURCE_CHANGE, TRANSCRIPTION_MODEL_CHANGE,
            };
            if AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
                || super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst)
            {
                break;
            }
        }

        // State machine transitions
        match audio_mode {
            AudioMode::Normal => {
                if mode_start.elapsed() >= NORMAL_DURATION {
                    audio_mode = AudioMode::Silence;
                    mode_start = Instant::now();
                    silence_buffer.clear();
                }
            }
            AudioMode::Silence => {
                if mode_start.elapsed() >= SILENCE_DURATION {
                    audio_mode = AudioMode::CatchUp;
                    mode_start = Instant::now();
                }
            }
            AudioMode::CatchUp => {
                if silence_buffer.is_empty() {
                    audio_mode = AudioMode::Normal;
                    mode_start = Instant::now();
                }
            }
        }

        // Send audio
        if last_send.elapsed() >= send_interval {
            let real_audio: Vec<i16> = {
                let mut buf = audio_buffer.lock().unwrap();
                std::mem::take(&mut *buf)
            };

            match audio_mode {
                AudioMode::Normal => {
                    if !real_audio.is_empty() {
                        if send_audio_chunk(&mut socket, &real_audio).is_err() {
                            break;
                        }
                        sent_chunks += 1;
                        sent_samples += real_audio.len();
                    }
                }
                AudioMode::Silence => {
                    silence_buffer.extend(real_audio);
                    let silence: Vec<i16> = vec![0i16; SAMPLES_PER_100MS];
                    if send_audio_chunk(&mut socket, &silence).is_err() {
                        break;
                    }
                    sent_chunks += 1;
                    sent_samples += silence.len();
                }
                AudioMode::CatchUp => {
                    silence_buffer.extend(real_audio);
                    let chunk_size = SAMPLES_PER_100MS * 2;
                    let to_send: Vec<i16> = if silence_buffer.len() >= chunk_size {
                        silence_buffer.drain(..chunk_size).collect()
                    } else if !silence_buffer.is_empty() {
                        std::mem::take(&mut silence_buffer)
                    } else {
                        Vec::new()
                    };
                    if !to_send.is_empty() && send_audio_chunk(&mut socket, &to_send).is_err() {
                        break;
                    }
                    if !to_send.is_empty() {
                        sent_chunks += 1;
                        sent_samples += to_send.len();
                    }
                }
            }
            last_send = Instant::now();
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
            }
        }

        // Receive transcriptions
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let msg = msg.as_str();
                if let Some(transcript) = parse_input_transcription(msg)
                    && !transcript.is_empty()
                {
                    last_transcription_time = Instant::now();
                    consecutive_empty_reads = 0;
                    transcript_updates += 1;
                    transcript_chars += transcript.chars().count();
                    let display_text = if let Ok(mut s) = state.lock() {
                        s.append_transcript(&transcript);
                        s.display_transcript.clone()
                    } else {
                        String::new()
                    };
                    if !display_text.is_empty() {
                        update_overlay_text(overlay_hwnd, &display_text);
                    }
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec())
                    && let Some(transcript) = parse_input_transcription(&text)
                    && !transcript.is_empty()
                {
                    last_transcription_time = Instant::now();
                    consecutive_empty_reads = 0;
                    transcript_updates += 1;
                    transcript_chars += transcript.chars().count();
                    let display_text = if let Ok(mut s) = state.lock() {
                        s.append_transcript(&transcript);
                        s.display_transcript.clone()
                    } else {
                        String::new()
                    };
                    if !display_text.is_empty() {
                        update_overlay_text(overlay_hwnd, &display_text);
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                crate::log_info!(
                    "[RealtimeGeminiLiveHealth] reconnect-start reason=close empty_reads={} since_transcript_ms={} mode={}",
                    consecutive_empty_reads,
                    last_transcription_time.elapsed().as_millis(),
                    audio_mode.as_str()
                );
                if !try_reconnect(ReconnectContext {
                    socket: &mut socket,
                    api_key: gemini_api_key,
                    model: gemini_live_model,
                    audio_buffer: &audio_buffer,
                    silence_buffer: &mut silence_buffer,
                    audio_mode: &mut audio_mode,
                    mode_start: &mut mode_start,
                    last_transcription_time: &mut last_transcription_time,
                    consecutive_empty_reads: &mut consecutive_empty_reads,
                }) {
                    crate::log_info!("[RealtimeGeminiLiveHealth] reconnect-failed reason=close");
                    break;
                }
                reconnect_count += 1;
                crate::log_info!(
                    "[RealtimeGeminiLiveHealth] reconnect-ok reason=close count={} catchup_ms={}",
                    reconnect_count,
                    samples_to_ms(silence_buffer.len())
                );
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                consecutive_empty_reads += 1;
                if consecutive_empty_reads >= EMPTY_READ_CHECK_COUNT
                    && last_transcription_time.elapsed()
                        > Duration::from_secs(NO_RESULT_THRESHOLD_SECS)
                {
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-start reason=no-results empty_reads={} since_transcript_ms={} mode={}",
                        consecutive_empty_reads,
                        last_transcription_time.elapsed().as_millis(),
                        audio_mode.as_str()
                    );
                    if !try_reconnect(ReconnectContext {
                        socket: &mut socket,
                        api_key: gemini_api_key,
                        model: gemini_live_model,
                        audio_buffer: &audio_buffer,
                        silence_buffer: &mut silence_buffer,
                        audio_mode: &mut audio_mode,
                        mode_start: &mut mode_start,
                        last_transcription_time: &mut last_transcription_time,
                        consecutive_empty_reads: &mut consecutive_empty_reads,
                    }) {
                        crate::log_info!(
                            "[RealtimeGeminiLiveHealth] reconnect-failed reason=no-results"
                        );
                        break;
                    }
                    reconnect_count += 1;
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-ok reason=no-results count={} catchup_ms={}",
                        reconnect_count,
                        samples_to_ms(silence_buffer.len())
                    );
                }
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("reset")
                    || error_str.contains("closed")
                    || error_str.contains("broken")
                {
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-start reason=socket-error error={} empty_reads={} since_transcript_ms={} mode={}",
                        error_str,
                        consecutive_empty_reads,
                        last_transcription_time.elapsed().as_millis(),
                        audio_mode.as_str()
                    );
                    if !try_reconnect(ReconnectContext {
                        socket: &mut socket,
                        api_key: gemini_api_key,
                        model: gemini_live_model,
                        audio_buffer: &audio_buffer,
                        silence_buffer: &mut silence_buffer,
                        audio_mode: &mut audio_mode,
                        mode_start: &mut mode_start,
                        last_transcription_time: &mut last_transcription_time,
                        consecutive_empty_reads: &mut consecutive_empty_reads,
                    }) {
                        crate::log_info!(
                            "[RealtimeGeminiLiveHealth] reconnect-failed reason=socket-error error={}",
                            error_str
                        );
                        break;
                    }
                    reconnect_count += 1;
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-ok reason=socket-error count={} catchup_ms={}",
                        reconnect_count,
                        samples_to_ms(silence_buffer.len())
                    );
                } else {
                    break;
                }
            }
        }

        if last_health_log.elapsed() >= Duration::from_secs(5) {
            crate::log_info!(
                "[RealtimeGeminiLiveHealth] uptime_ms={} model={} mode={} mode_ms={} sent_chunks={} sent_ms={} catchup_ms={} transcript_updates={} transcript_chars={} empty_reads={} since_transcript_ms={} reconnects={}",
                session_started.elapsed().as_millis(),
                gemini_live_model,
                audio_mode.as_str(),
                mode_start.elapsed().as_millis(),
                sent_chunks,
                samples_to_ms(sent_samples),
                samples_to_ms(silence_buffer.len()),
                transcript_updates,
                transcript_chars,
                consecutive_empty_reads,
                last_transcription_time.elapsed().as_millis(),
                reconnect_count
            );
            last_health_log = Instant::now();
            sent_chunks = 0;
            sent_samples = 0;
            transcript_updates = 0;
            transcript_chars = 0;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = socket.close(None);
    Ok(())
}

struct ReconnectContext<'a> {
    socket: &'a mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    api_key: &'a str,
    model: &'a str,
    audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    silence_buffer: &'a mut Vec<i16>,
    audio_mode: &'a mut AudioMode,
    mode_start: &'a mut Instant,
    last_transcription_time: &'a mut Instant,
    consecutive_empty_reads: &'a mut u32,
}

fn try_reconnect(context: ReconnectContext<'_>) -> bool {
    let ReconnectContext {
        socket,
        api_key,
        model,
        audio_buffer,
        silence_buffer,
        audio_mode,
        mode_start,
        last_transcription_time,
        consecutive_empty_reads,
    } = context;
    let mut reconnect_buffer: Vec<i16> = Vec::new();
    let _ = socket.close(None);

    for _attempt in 1..=3 {
        {
            let mut buf = audio_buffer.lock().unwrap();
            reconnect_buffer.extend(std::mem::take(&mut *buf));
        }

        match connect_websocket(api_key) {
            Ok(mut new_socket) => {
                if send_setup_message(&mut new_socket, model).is_err() {
                    continue;
                }
                if set_socket_nonblocking(&mut new_socket).is_err() {
                    continue;
                }
                {
                    let mut buf = audio_buffer.lock().unwrap();
                    reconnect_buffer.extend(std::mem::take(&mut *buf));
                }
                silence_buffer.clear();
                silence_buffer.extend(reconnect_buffer);
                *audio_mode = AudioMode::CatchUp;
                *mode_start = Instant::now();
                *socket = new_socket;
                *last_transcription_time = Instant::now();
                *consecutive_empty_reads = 0;
                return true;
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    false
}
