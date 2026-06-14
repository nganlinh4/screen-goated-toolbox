use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};
use tungstenite::Message;

use super::super::manager::TtsManager;
use super::super::types::{
    AudioEvent, SOURCE_SAMPLE_RATE, TtsCollectedAudio, TtsRequestProfile,
};
use super::super::utils::{
    clear_tts_loading_state, clear_tts_state, get_language_instruction_for_code,
    get_language_instruction_for_text,
};
use super::super::websocket::{
    connect_tts_websocket, is_turn_complete, parse_audio_data, send_tts_setup, send_tts_text,
};
use super::{acquire_warm_socket, start_warm_up};

use crate::APP;

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

    let mut socket = connect_tts_websocket(&api_key)?;
    let model = crate::model_config::normalize_tts_gemini_model(&profile.gemini_model).to_string();
    let instruction = playground_language_instruction(text, &profile);
    send_tts_setup(
        &mut socket,
        &model,
        &profile.gemini_voice,
        &profile.gemini_speed,
        instruction.as_deref(),
    )?;

    let setup_started = Instant::now();
    let mut setup_complete = false;
    while !cancel.load(Ordering::SeqCst) {
        if setup_started.elapsed() > Duration::from_secs(15) {
            break;
        }
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let msg = msg.as_str();
                if msg.contains("setupComplete") {
                    setup_complete = true;
                    break;
                }
                if msg.contains("error") || msg.contains("Error") {
                    let _ = socket.close(None);
                    return Err(anyhow::anyhow!("Gemini setup error: {msg}"));
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(msg) = String::from_utf8(data.to_vec())
                    && msg.contains("setupComplete")
                {
                    setup_complete = true;
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => {
                let _ = socket.close(None);
                return Err(err.into());
            }
        }
    }
    if cancel.load(Ordering::SeqCst) {
        let _ = socket.close(None);
        return Err(anyhow::anyhow!("Generation cancelled"));
    }
    if !setup_complete {
        let _ = socket.close(None);
        return Err(anyhow::anyhow!("Gemini setup timeout"));
    }

    send_tts_text(&mut socket, text)?;
    let read_started = Instant::now();
    let mut audio_bytes = Vec::new();
    while !cancel.load(Ordering::SeqCst) {
        if read_started.elapsed() > Duration::from_secs(90) {
            let _ = socket.close(None);
            return Err(anyhow::anyhow!("TTS generation timed out"));
        }
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let msg = msg.as_str();
                if let Some(audio_data) = parse_audio_data(msg) {
                    audio_bytes.extend_from_slice(&audio_data);
                }
                if is_turn_complete(msg) {
                    break;
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(msg) = String::from_utf8(data.to_vec()) {
                    if let Some(audio_data) = parse_audio_data(&msg) {
                        audio_bytes.extend_from_slice(&audio_data);
                    }
                    if is_turn_complete(&msg) {
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                let _ = socket.close(None);
                return Err(err.into());
            }
        }
    }
    let _ = socket.close(None);
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
    let mut socket = if let Some(warm) = acquire_warm_socket(&api_key) {
        warm
    } else {
        eprintln!("[TTS Worker] Connecting to Gemini WebSocket...");
        let socket_result = connect_tts_websocket(&api_key);
        match socket_result {
            Ok(s) => {
                eprintln!("[TTS Worker] WebSocket connected successfully");
                s
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

    let mut setup_complete = false;
    if let Err(e) = send_tts_setup(
        &mut socket,
        &current_model,
        &current_voice,
        &current_speed,
        language_instruction.as_deref(),
    ) {
        eprintln!("TTS: Failed to send setup: {}", e);
        let _ = socket.close(None);
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        std::thread::sleep(Duration::from_secs(2));
        return;
    }

    // Wait for setup acknowledgment
    let setup_start = Instant::now();
    loop {
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst)
            || manager.shutdown.load(Ordering::SeqCst)
        {
            let _ = socket.close(None);
            let _ = tx.send(AudioEvent::End);
            break;
        }

        match socket.read() {
            Ok(Message::Text(msg)) => {
                let msg = msg.as_str();
                if msg.contains("setupComplete") {
                    setup_complete = true;
                    break;
                }
                if msg.contains("error") || msg.contains("Error") {
                    eprintln!("TTS: Setup error: {}", msg);
                    break;
                }
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec())
                    && text.contains("setupComplete")
                {
                    setup_complete = true;
                    break;
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if setup_start.elapsed() > Duration::from_secs(10) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                break;
            }
        }
    }

    if manager.shutdown.load(Ordering::SeqCst) {
        return;
    }

    if !setup_complete {
        let _ = socket.close(None);
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    // Send request text
    if let Err(e) = send_tts_text(&mut socket, &request.req.text) {
        eprintln!("TTS: Failed to send text: {}", e);
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        let _ = socket.close(None);
        return;
    }

    // Read loop
    loop {
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst)
            || manager.shutdown.load(Ordering::SeqCst)
        {
            let _ = socket.close(None);
            let _ = tx.send(AudioEvent::End);
            break;
        }

        match socket.read() {
            Ok(Message::Text(msg)) => {
                let msg = msg.as_str();
                if let Some(audio_data) = parse_audio_data(msg) {
                    let _ = tx.send(AudioEvent::Data(audio_data));
                }
                if is_turn_complete(msg) {
                    let _ = tx.send(AudioEvent::End);
                    break;
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    if let Some(audio_data) = parse_audio_data(&text) {
                        let _ = tx.send(AudioEvent::Data(audio_data));
                    }
                    if is_turn_complete(&text) {
                        let _ = tx.send(AudioEvent::End);
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                let _ = tx.send(AudioEvent::End);
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
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

    let _ = socket.close(None);

    // Pre-connect next warm socket for subsequent requests
    start_warm_up(api_key.clone());
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
