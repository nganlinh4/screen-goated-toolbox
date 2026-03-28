use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};
use tungstenite::Message;

use super::manager::TtsManager;
use super::types::AudioEvent;
use super::utils::{clear_tts_loading_state, clear_tts_state, get_language_instruction_for_text};
use super::websocket::{
    connect_tts_websocket, is_turn_complete, parse_audio_data, send_tts_setup, send_tts_text,
};

use crate::APP;

mod worker_edge;
mod worker_google;

// ---- Warm socket pool for instant TTS ----
use native_tls::TlsStream;
use std::net::TcpStream;
use std::sync::Mutex;
use tungstenite::WebSocket;

struct WarmSocket {
    socket: WebSocket<TlsStream<TcpStream>>,
    created_at: Instant,
    api_key: String,
}

lazy_static::lazy_static! {
    static ref WARM_SOCKET: Mutex<Option<WarmSocket>> = Mutex::new(None);
}

const WARM_SOCKET_MAX_AGE: Duration = Duration::from_secs(86400);

fn acquire_warm_socket(api_key: &str) -> Option<WebSocket<TlsStream<TcpStream>>> {
    let mut guard = WARM_SOCKET.lock().ok()?;
    let mut warm = guard.take()?;
    if warm.api_key != api_key || warm.created_at.elapsed() > WARM_SOCKET_MAX_AGE {
        let _ = warm.socket.close(None);
        return None;
    }
    eprintln!("[TTS Worker] Using WARM socket (0ms connect+setup)");
    Some(warm.socket)
}

pub fn start_warm_up_public(api_key: String) {
    start_warm_up(api_key);
}

fn start_warm_up(api_key: String) {
    std::thread::spawn(move || {
        let start = Instant::now();
        match connect_tts_websocket(&api_key) {
            Ok(socket) => {
                let elapsed = start.elapsed().as_millis();
                eprintln!("[TTS Worker] Warm socket ready in {}ms", elapsed);
                if let Ok(mut guard) = WARM_SOCKET.lock() {
                    if let Some(mut old) = guard.take() {
                        let _ = old.socket.close(None);
                    }
                    *guard = Some(WarmSocket {
                        socket,
                        created_at: Instant::now(),
                        api_key,
                    });
                }
            }
            Err(e) => {
                eprintln!("[TTS Worker] Warm-up connect failed: {}", e);
            }
        }
    });
}

/// Socket Worker thread - fetches audio data and pipes it to the player
pub fn run_socket_worker(manager: Arc<TtsManager>) {
    std::thread::sleep(Duration::from_millis(100));

    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }

        // Wait for a request
        let (request, tx) = {
            let mut queue = manager.work_queue.lock().unwrap();
            while queue.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                let result = manager.work_signal.wait(queue).unwrap();
                queue = result;
            }
            if manager.shutdown.load(Ordering::SeqCst) {
                return;
            }
            queue.pop_front().unwrap()
        };

        // Check if this request is stale
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst) {
            eprintln!(
                "[TTS Worker] Request stale (gen {} < current {}), skipping",
                request.generation,
                manager.interrupt_generation.load(Ordering::SeqCst)
            );
            let _ = tx.send(AudioEvent::End);
            continue;
        }

        eprintln!(
            "[TTS Worker] Processing request: hwnd={}, text_len={}, realtime={}",
            request.req.hwnd,
            request.req.text.len(),
            request.req.is_realtime
        );

        // Check TTS Method
        let tts_method = {
            match APP.lock() {
                Ok(app) => app.config.tts_method.clone(),
                Err(e) => {
                    eprintln!("[TTS Worker] ERROR: Failed to lock APP config: {:?}", e);
                    let _ = tx.send(AudioEvent::End);
                    continue;
                }
            }
        };

        eprintln!("[TTS Worker] Using TTS method: {:?}", tts_method);

        if tts_method == crate::config::TtsMethod::GoogleTranslate {
            eprintln!("[TTS Worker] Routing to Google Translate TTS");
            worker_google::handle_google_tts(manager.clone(), request, tx);
            continue;
        }

        if tts_method == crate::config::TtsMethod::EdgeTTS {
            eprintln!("[TTS Worker] Routing to Edge TTS");
            worker_edge::handle_edge_tts(manager.clone(), request, tx);
            continue;
        }

        eprintln!("[TTS Worker] Using Gemini TTS");
        handle_gemini_tts(&manager, request, tx);
    }
}

fn handle_gemini_tts(
    manager: &Arc<TtsManager>,
    request: super::types::QueuedRequest,
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

    let (current_voice, current_speed, language_instruction) = {
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
    let current_model = {
        let app = APP.lock().unwrap();
        let model = app.config.tts_gemini_live_model.trim();
        if model.is_empty() {
            crate::model_config::DEFAULT_GEMINI_LIVE_TTS_MODEL.to_string()
        } else {
            app.config.tts_gemini_live_model.clone()
        }
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

pub(super) fn resample_audio(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = to_rate as f32 / from_rate as f32;
    let new_len = (samples.len() as f32 * ratio) as usize;
    let mut result = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx_f = i as f32 / ratio;
        let src_idx = src_idx_f as usize;

        if src_idx >= samples.len() - 1 {
            result.push(samples[src_idx.min(samples.len() - 1)]);
        } else {
            let t = src_idx_f - src_idx as f32;
            let s1 = samples[src_idx] as f32;
            let s2 = samples[src_idx + 1] as f32;
            let val = s1 + t * (s2 - s1);
            result.push(val as i16);
        }
    }

    result
}
