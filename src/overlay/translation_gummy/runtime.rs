use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::APP;
use crate::api::realtime_audio::start_mic_capture;
use crate::api::realtime_audio::websocket::{
    connect_websocket, send_audio_chunk, send_audio_stream_end, set_socket_nonblocking,
    set_socket_short_timeout,
};
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;
use crate::config::TranslationGummySettings;
use base64::{Engine as _, engine::general_purpose};
use serde::Serialize;
use tungstenite::Message;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationGummyConnectionState {
    NotConfigured,
    Connecting,
    Ready,
    Reconnecting,
    Error,
    Stopped,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationGummyTranscriptItem {
    pub id: u64,
    pub role: &'static str,
    pub text: String,
    pub is_final: bool,
    pub lang: String,
}

struct SessionControl {
    stop: Arc<std::sync::atomic::AtomicBool>,
}

static TRANSCRIPT_COUNTER: AtomicU64 = AtomicU64::new(1);
static PLAYBACK_COUNTER: AtomicU64 = AtomicU64::new(1);

lazy_static::lazy_static! {
    static ref SESSION_CONTROL: Mutex<Option<SessionControl>> = Mutex::new(None);
}

struct LocalInputTurnState {
    pre_roll: Vec<i16>,
    turn_active: bool,
    last_speech_at: Option<Instant>,
}

pub fn start_session(hwnd: isize, settings: TranslationGummySettings) {
    stop_session();

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    *SESSION_CONTROL.lock().unwrap() = Some(SessionControl { stop: stop.clone() });

    std::thread::spawn(move || run_loop(hwnd, settings, stop));
}

pub fn stop_session() {
    if let Some(control) = SESSION_CONTROL.lock().unwrap().take() {
        control.stop.store(true, Ordering::SeqCst);
    }
    TTS_MANAGER.stop();
}

fn run_loop(
    hwnd: isize,
    settings: TranslationGummySettings,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    let api_key = match APP.lock() {
        Ok(app) => app.config.gemini_api_key.trim().to_string(),
        Err(_) => String::new(),
    };
    if api_key.is_empty() {
        super::publish_error(
            TranslationGummyConnectionState::Error,
            "missing_api_key".to_string(),
            false,
        );
        return;
    }

    let mut reconnecting = false;
    while !stop.load(Ordering::SeqCst) {
        super::publish_connection(
            if reconnecting {
                TranslationGummyConnectionState::Reconnecting
            } else {
                TranslationGummyConnectionState::Connecting
            },
            true,
            None,
        );

        let result = run_single_session(hwnd, &api_key, &settings, stop.clone());
        if stop.load(Ordering::SeqCst) {
            break;
        }

        match result {
            Ok(()) => break,
            Err(error) => {
                let msg = error.to_string();
                let is_normal_close = msg.contains("closed (1000)")
                    || msg.contains("closed (1001)")
                    || msg.contains("closed normally");
                if is_normal_close {
                    // Server-side session timeout — reconnect silently
                    super::publish_connection(
                        TranslationGummyConnectionState::Reconnecting,
                        true,
                        None,
                    );
                    std::thread::sleep(Duration::from_millis(500));
                } else {
                    super::publish_error(TranslationGummyConnectionState::Error, msg, false);
                    std::thread::sleep(Duration::from_millis(1200));
                }
                reconnecting = true;
            }
        }
    }

    if !stop.load(Ordering::SeqCst) {
        super::publish_connection(TranslationGummyConnectionState::Stopped, false, None);
    }
}

fn run_single_session(
    hwnd: isize,
    api_key: &str,
    settings: &TranslationGummySettings,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<()> {
    let mut socket = connect_websocket(api_key)?;
    send_setup(&mut socket, settings)?;
    set_socket_short_timeout(&mut socket)?;
    wait_for_setup(&mut socket, stop.clone())?;
    set_socket_nonblocking(&mut socket)?;

    super::insert_session_separator();
    super::publish_connection(TranslationGummyConnectionState::Ready, true, None);
    super::publish_audio_level(0.0);

    let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
    let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stream = start_mic_capture(buffer.clone(), stop.clone(), pause)?;
    let _keep_stream_alive = stream;

    let mut playback = PlaybackBridge::new(hwnd);
    let mut pending_audio: Vec<i16> = Vec::new();
    let mut input_turn = LocalInputTurnState {
        pre_roll: Vec::new(),
        turn_active: false,
        last_speech_at: None,
    };

    while !stop.load(Ordering::SeqCst) {
        flush_audio(&mut socket, &buffer, &mut pending_audio, &mut input_turn)?;

        match socket.read() {
            Ok(Message::Text(msg)) => {
                handle_update(&msg, hwnd, &mut playback)?;
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    handle_update(&text, hwnd, &mut playback)?;
                }
            }
            Ok(Message::Close(frame)) => {
                let detail = frame
                    .map(|f| {
                        if f.reason.is_empty() {
                            format!("connection closed ({})", f.code)
                        } else {
                            format!("connection closed ({}: {})", f.code, f.reason)
                        }
                    })
                    .unwrap_or_else(|| "connection closed".to_string());
                playback.end();
                return Err(anyhow::anyhow!(detail));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(err) => {
                playback.end();
                return Err(err.into());
            }
        }
    }

    let _ = send_audio_stream_end(&mut socket);
    let _ = socket.close(None);
    playback.end();
    super::finalize_transcripts();
    super::publish_connection(TranslationGummyConnectionState::Stopped, false, None);
    Ok(())
}

fn send_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    settings: &TranslationGummySettings,
) -> anyhow::Result<()> {
    let (model_name, voice_name) = current_gemini_tts_settings();
    let payload = serde_json::json!({
        "setup": {
            "model": format!("models/{}", model_name),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "mediaResolution": "MEDIA_RESOLUTION_LOW",
                "thinkingConfig": { "thinkingLevel": "minimal" },
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": voice_name
                        }
                    }
                }
            },
            "systemInstruction": {
                "parts": [{ "text": settings.build_system_instruction() }]
            },
            "realtimeInputConfig": {
                "automaticActivityDetection": {
                    "startOfSpeechSensitivity": "START_SENSITIVITY_HIGH",
                    "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH",
                    "prefixPaddingMs": 80,
                    "silenceDurationMs": 320
                },
                "activityHandling": "START_OF_ACTIVITY_INTERRUPTS",
                "turnCoverage": "TURN_INCLUDES_ONLY_ACTIVITY"
            },
            "contextWindowCompression": {
                "slidingWindow": {}
            },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {}
        }
    });

    socket.write(Message::Text(payload.to_string().into()))?;
    socket.flush()?;
    Ok(())
}

pub(super) fn current_gemini_tts_settings() -> (String, String) {
    APP.lock()
        .map(|app| {
            let model = app.config.tts_gemini_live_model.trim();
            let voice = app.config.tts_voice.trim();
            (
                if model.is_empty() {
                    "gemini-3.1-flash-live-preview".to_string()
                } else {
                    model.to_string()
                },
                if voice.is_empty() {
                    "Aoede".to_string()
                } else {
                    voice.to_string()
                },
            )
        })
        .unwrap_or_else(|_| {
            (
                "gemini-3.1-flash-live-preview".to_string(),
                "Aoede".to_string(),
            )
        })
}

fn wait_for_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let update = parse_update(msg.as_str());
                if let Some(error) = update.error {
                    return Err(anyhow::anyhow!(error));
                }
                if update.setup_complete {
                    return Ok(());
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let update = parse_update(&text);
                    if let Some(error) = update.error {
                        return Err(anyhow::anyhow!(error));
                    }
                    if update.setup_complete {
                        return Ok(());
                    }
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if started.elapsed() > Duration::from_secs(15) {
                    return Err(anyhow::anyhow!("setup timeout"));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(err.into()),
        }
    }

    Err(anyhow::anyhow!("stopped"))
}

fn flush_audio(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    buffer: &Arc<Mutex<Vec<i16>>>,
    pending_audio: &mut Vec<i16>,
    input_turn: &mut LocalInputTurnState,
) -> anyhow::Result<()> {
    {
        let mut guard = buffer.lock().unwrap();
        if !guard.is_empty() {
            pending_audio.extend(guard.drain(..));
        }
    }

    const CHUNK_SAMPLES: usize = 1600;
    while pending_audio.len() >= CHUNK_SAMPLES {
        let chunk: Vec<i16> = pending_audio.drain(..CHUNK_SAMPLES).collect();
        let rms = calculate_rms(&chunk);
        super::publish_audio_level(calculate_audio_level(&chunk));
        if rms >= LOCAL_INPUT_SPEECH_RMS {
            if !input_turn.turn_active {
                if !input_turn.pre_roll.is_empty() {
                    send_audio_chunk(socket, &input_turn.pre_roll)?;
                    input_turn.pre_roll.clear();
                }
                input_turn.turn_active = true;
            }
            input_turn.last_speech_at = Some(Instant::now());
            send_audio_chunk(socket, &chunk)?;
            continue;
        }

        if !input_turn.turn_active {
            input_turn.pre_roll.extend_from_slice(&chunk);
            if input_turn.pre_roll.len() > LOCAL_INPUT_PREROLL_SAMPLES {
                let overflow = input_turn.pre_roll.len() - LOCAL_INPUT_PREROLL_SAMPLES;
                input_turn.pre_roll.drain(..overflow);
            }
            continue;
        }

        let silence_ms = input_turn
            .last_speech_at
            .map(|started| started.elapsed().as_millis() as u64)
            .unwrap_or(LOCAL_INPUT_END_SILENCE_MS);
        if silence_ms <= LOCAL_INPUT_TRAILING_AUDIO_MS {
            send_audio_chunk(socket, &chunk)?;
            continue;
        }
        if silence_ms >= LOCAL_INPUT_END_SILENCE_MS {
            send_audio_stream_end(socket)?;
            input_turn.turn_active = false;
            input_turn.last_speech_at = None;
            input_turn.pre_roll.clear();
        }
    }
    Ok(())
}

fn calculate_audio_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    let rms = (sum_squares / samples.len() as f32).sqrt();
    (rms * 5.5).clamp(0.0, 1.0)
}

fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

fn handle_update(message: &str, hwnd: isize, playback: &mut PlaybackBridge) -> anyhow::Result<()> {
    let update = parse_update(message);
    if let Some(error) = update.error {
        return Err(anyhow::anyhow!(error));
    }

    if let Some(text) = update.input_transcript {
        super::upsert_transcript("input", text, update.turn_complete);
    }
    if let Some(text) = update.output_transcript {
        super::upsert_transcript("output", text, update.turn_complete);
    }
    if let Some(audio) = update.audio_chunk {
        playback.push(audio);
    }
    if update.interrupted {
        super::finalize_transcripts();
        playback.interrupt(hwnd);
    } else if update.turn_complete {
        super::finalize_transcripts();
    }
    if update.go_away {
        // Server is about to terminate — trigger clean reconnect
        return Err(anyhow::anyhow!("connection closed (1001)"));
    }

    Ok(())
}

struct PlaybackBridge {
    tx: mpsc::Sender<AudioEvent>,
}

impl PlaybackBridge {
    fn new(hwnd: isize) -> Self {
        let (tx, rx) = mpsc::channel();
        let generation = TTS_MANAGER.interrupt_generation.load(Ordering::SeqCst);
        let request_id = PLAYBACK_COUNTER.fetch_add(1, Ordering::SeqCst);
        {
            let mut queue = TTS_MANAGER.playback_queue.lock().unwrap();
            queue.push_back((rx, hwnd, request_id, generation, false));
        }
        TTS_MANAGER.playback_signal.notify_one();
        Self { tx }
    }

    fn push(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(AudioEvent::Data(bytes));
    }

    fn end(&self) {
        let _ = self.tx.send(AudioEvent::End);
    }

    fn interrupt(&mut self, hwnd: isize) {
        TTS_MANAGER.stop();
        *self = Self::new(hwnd);
    }
}

struct ParsedUpdate {
    setup_complete: bool,
    input_transcript: Option<String>,
    output_transcript: Option<String>,
    audio_chunk: Option<Vec<u8>>,
    turn_complete: bool,
    interrupted: bool,
    error: Option<String>,
    go_away: bool,
}

fn parse_update(message: &str) -> ParsedUpdate {
    let mut update = ParsedUpdate {
        setup_complete: false,
        input_transcript: None,
        output_transcript: None,
        audio_chunk: None,
        turn_complete: false,
        interrupted: false,
        error: None,
        go_away: false,
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(message) else {
        return update;
    };

    if message.contains("setupComplete") {
        update.setup_complete = true;
    }

    // GoAway: server signals imminent termination — reconnect gracefully
    if json.get("goAway").is_some() {
        update.go_away = true;
        return update;
    }

    if let Some(error) = json.get("error") {
        update.error = error
            .get("message")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| Some(error.to_string()));
        return update;
    }

    let Some(server_content) = json.get("serverContent") else {
        return update;
    };

    if server_content
        .get("turnComplete")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || server_content
            .get("generationComplete")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    {
        update.turn_complete = true;
    }
    update.interrupted = server_content
        .get("interrupted")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    update.input_transcript = server_content
        .get("inputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    update.output_transcript = server_content
        .get("outputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(parts) = server_content
        .get("modelTurn")
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
    {
        for part in parts {
            if update.audio_chunk.is_none()
                && let Some(inline) = part.get("inlineData")
                && let Some(data) = inline.get("data").and_then(|value| value.as_str())
                && let Ok(bytes) = general_purpose::STANDARD.decode(data)
            {
                update.audio_chunk = Some(bytes);
            }
        }
    }

    update
}

const LOCAL_INPUT_SPEECH_RMS: f32 = 0.015;
const LOCAL_INPUT_TRAILING_AUDIO_MS: u64 = 180;
const LOCAL_INPUT_END_SILENCE_MS: u64 = 420;
const LOCAL_INPUT_PREROLL_SAMPLES: usize = 3200;

pub fn next_transcript_id() -> u64 {
    TRANSCRIPT_COUNTER.fetch_add(1, Ordering::SeqCst)
}
