//! Translation Gummy session runtime.
//!
//! [`mod.rs`] owns the public API and the cohesive session/reconnect loop:
//! [`run_loop`] retries [`run_single_session`], which runs the main websocket
//! read/write loop. The satellite concerns live in submodules:
//! - [`setup`] — setup-payload handshake (`send_setup`/`wait_for_setup`).
//! - [`audio`] — local mic plumbing + VAD (`flush_audio`).
//! - [`protocol`] — server-frame decoding + playback bridge (`handle_update`).

mod audio;
mod protocol;
mod setup;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use crate::APP;
use crate::api::gemini_live::transport::{
    connect_websocket, set_socket_nonblocking, set_socket_short_timeout,
};
use crate::api::realtime_audio::start_mic_capture;
use crate::api::realtime_audio::websocket::send_audio_stream_end;
use crate::api::tts::TTS_MANAGER;
use crate::config::TranslationGummySettings;
use serde::Serialize;
use tungstenite::Message;

use audio::{LocalInputTurnState, flush_audio};
use protocol::{PlaybackBridge, handle_update};
use setup::{send_setup, wait_for_setup};

pub(in crate::overlay::translation_gummy) use setup::current_gemini_tts_settings;

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

static SESSION_CONTROL: LazyLock<Mutex<Option<SessionControl>>> =
    LazyLock::new(|| Mutex::new(None));

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
    let mut input_turn = LocalInputTurnState::new();

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

pub fn next_transcript_id() -> u64 {
    TRANSCRIPT_COUNTER.fetch_add(1, Ordering::SeqCst)
}
