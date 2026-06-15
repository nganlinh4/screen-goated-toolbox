use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

use super::manager::TtsManager;
use super::types::AudioEvent;
use super::websocket::connect_tts_websocket;

use crate::APP;

mod audio_utils;
mod gemini;
mod open_weights;
mod sidecar;
mod worker_edge;
mod worker_google;
mod worker_kokoro;
mod worker_magpie;
mod worker_step_audio;
mod worker_supertonic;
mod worker_vieneu;
mod worker_voxtral;

pub(crate) use audio_utils::resample_audio;
pub use gemini::synthesize_gemini_live_to_wav_cancel;

// ---- Warm socket pool for instant TTS ----
use native_tls::TlsStream;
use std::net::TcpStream;
use std::sync::{LazyLock, Mutex};
use tungstenite::WebSocket;

struct WarmSocket {
    socket: WebSocket<TlsStream<TcpStream>>,
    created_at: Instant,
    api_key: String,
}

static WARM_SOCKET: LazyLock<Mutex<Option<WarmSocket>>> = LazyLock::new(|| Mutex::new(None));

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

pub fn synthesize_step_audio_edit_to_wav_cancel(
    source_audio_path: String,
    source_text: String,
    edit_type: String,
    edit_info: String,
    target_text: String,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<super::types::TtsCollectedAudio> {
    worker_step_audio::synthesize_step_audio_edit_to_wav(
        source_audio_path,
        source_text,
        edit_type,
        edit_info,
        target_text,
        Some(cancel),
    )
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
        let tts_method = if let Some(profile) = &request.req.profile {
            profile.method.clone()
        } else {
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

        match tts_method {
            crate::config::TtsMethod::GoogleTranslate => {
                eprintln!("[TTS Worker] Routing to Google Translate TTS");
                worker_google::handle_google_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::EdgeTTS => {
                eprintln!("[TTS Worker] Routing to Edge TTS");
                worker_edge::handle_edge_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::FishAudioS2Pro => {
                eprintln!("[TTS Worker] Fish Audio S2 Pro has been removed");
                open_weights::fail_request(
                    "Fish Audio S2 Pro",
                    request.req.hwnd,
                    &tx,
                    "Fish Audio S2 Pro was removed because it requires workstation-class GPU memory.",
                );
            }
            crate::config::TtsMethod::StepAudioEditX => {
                eprintln!("[TTS Worker] Routing to Step Audio EditX");
                worker_step_audio::handle_step_audio_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::MagpieMultilingual => {
                eprintln!("[TTS Worker] Routing to NVIDIA Magpie-Multilingual");
                worker_magpie::handle_magpie_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::Kokoro => {
                eprintln!("[TTS Worker] Routing to Kokoro 82M v1.0");
                worker_kokoro::handle_kokoro_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::Supertonic => {
                eprintln!("[TTS Worker] Routing to Supertonic 3");
                worker_supertonic::handle_supertonic_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::VieneuTts => {
                eprintln!("[TTS Worker] Routing to VieNeu-TTS v2");
                worker_vieneu::handle_vieneu_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::VoxtralTts => {
                eprintln!("[TTS Worker] Routing to Mistral Voxtral TTS");
                worker_voxtral::handle_voxtral_tts(manager.clone(), request, tx);
            }
            crate::config::TtsMethod::GeminiLive => {
                eprintln!("[TTS Worker] Using Gemini TTS");
                gemini::handle_gemini_tts(&manager, request, tx);
            }
        }
    }
}
