//! Translation Gummy session runtime.
//!
//! [`mod.rs`] owns the public API and the cohesive session/reconnect loop:
//! [`run_loop`] retries [`run_single_session`], which runs the main websocket
//! read/write loop. The satellite concerns live in submodules:
//! - [`setup`] — setup-payload handshake (`send_setup`/`wait_for_setup`).
//! - [`audio`] — local mic plumbing + VAD (`flush_audio`).
//! - [`protocol`] — server-frame decoding + playback bridge (`handle_update`).

mod audio;
#[cfg(test)]
mod audio_publication_tests;
mod maintenance;
mod protocol;
#[cfg(test)]
mod retry_tests;
mod setup;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::APP;
use crate::api::gemini_live::lifecycle::{
    LiveBackoffPolicy, LiveLifecycleEffect, LiveLifecycleEvent, LiveLifecycleFrame,
    LiveLifecyclePolicy, LiveSessionLifecycle,
};
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

pub(super) use maintenance::request_persist;

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
    playback_ids: Arc<Mutex<Vec<u64>>>,
}

static TRANSCRIPT_COUNTER: AtomicU64 = AtomicU64::new(1);

static SESSION_CONTROL: LazyLock<Mutex<Option<SessionControl>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug)]
enum SessionFailure {
    Server { retryable: bool },
    Close(u16),
    Audio,
    SetupOverflow,
}

impl std::fmt::Display for SessionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Live session failed")
    }
}

impl std::error::Error for SessionFailure {}

fn retryable(error: &anyhow::Error) -> bool {
    if let Some(failure) = error.downcast_ref::<SessionFailure>() {
        return match failure {
            SessionFailure::Server { retryable } => *retryable,
            SessionFailure::Close(code) => !matches!(code, 1008 | 1002 | 1003 | 1007 | 1009),
            SessionFailure::Audio | SessionFailure::SetupOverflow => false,
        };
    }
    if let Some(tungstenite::Error::Http(response)) = error.downcast_ref::<tungstenite::Error>() {
        let status = response.status().as_u16();
        return status == 408 || status == 429 || status >= 500;
    }
    true
}

fn publish_owned(owner: &Arc<std::sync::atomic::AtomicBool>, update: impl FnOnce()) -> bool {
    publish_owned_with(
        &SESSION_CONTROL,
        owner,
        update,
        UiUpdate::FullState,
        request_update,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiUpdate {
    FullState,
    AudioLevel,
}

fn request_update(update: UiUpdate) {
    match update {
        UiUpdate::FullState => super::state::request_sync(),
        UiUpdate::AudioLevel => super::state::meter::request(),
    }
}

fn publish_owned_with(
    control: &Mutex<Option<SessionControl>>,
    owner: &Arc<std::sync::atomic::AtomicBool>,
    update: impl FnOnce(),
    kind: UiUpdate,
    notify: impl FnOnce(UiUpdate),
) -> bool {
    let control = control.lock().unwrap();
    if !accepts_owner(control.as_ref(), owner) {
        return false;
    }
    update();
    // Keep the owner through the nonblocking notification, so retirement cannot
    // overtake an admitted publication and queue it into a replacement session.
    notify(kind);
    true
}

fn publish_audio_owned(owner: &Arc<std::sync::atomic::AtomicBool>, level: f32) -> bool {
    publish_audio_owned_with(
        &SESSION_CONTROL,
        owner,
        || super::state::publish_audio_level(level),
        request_update,
    )
}

fn publish_audio_owned_with(
    control: &Mutex<Option<SessionControl>>,
    owner: &Arc<std::sync::atomic::AtomicBool>,
    update: impl FnOnce(),
    notify: impl FnOnce(UiUpdate),
) -> bool {
    publish_owned_with(control, owner, update, UiUpdate::AudioLevel, notify)
}

fn accepts_owner(
    control: Option<&SessionControl>,
    owner: &Arc<std::sync::atomic::AtomicBool>,
) -> bool {
    control.is_some_and(|current| Arc::ptr_eq(&current.stop, owner))
        && !owner.load(Ordering::SeqCst)
}

pub fn start_session(hwnd: isize, settings: TranslationGummySettings) {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let playback_ids = Arc::new(Mutex::new(Vec::new()));
    let (old, pending) = {
        let mut current = SESSION_CONTROL.lock().unwrap();
        let mut pending = Vec::new();
        if let Some(previous) = current.as_ref() {
            previous.stop.store(true, Ordering::SeqCst);
            pending = super::state::finalize_transcripts();
        }
        let old = current.replace(SessionControl {
            stop: stop.clone(),
            playback_ids: playback_ids.clone(),
        });
        (old, pending)
    };
    if let Some(old) = old {
        retire_control(old);
        request_persist();
    }
    maintenance::request_detection(pending);

    std::thread::spawn(move || run_loop(hwnd, settings, stop, playback_ids));
}

pub fn stop_session() {
    let (control, pending) = {
        let mut current = SESSION_CONTROL.lock().unwrap();
        let mut pending = Vec::new();
        if let Some(previous) = current.as_ref() {
            previous.stop.store(true, Ordering::SeqCst);
            pending = super::state::finalize_transcripts();
        }
        (current.take(), pending)
    };
    if let Some(control) = control {
        retire_control(control);
        request_persist();
    }
    maintenance::request_detection(pending);
}

fn retire_control(control: SessionControl) {
    control.stop.store(true, Ordering::SeqCst);
    stop_playback(&control.playback_ids);
}

fn stop_playback(playback_ids: &Arc<Mutex<Vec<u64>>>) {
    let ids: Vec<_> = playback_ids.lock().unwrap().drain(..).collect();
    for id in ids {
        TTS_MANAGER.stop_if_active(id);
    }
}

fn run_loop(
    hwnd: isize,
    settings: TranslationGummySettings,
    stop: Arc<std::sync::atomic::AtomicBool>,
    playback_ids: Arc<Mutex<Vec<u64>>>,
) {
    let api_key = match APP.lock() {
        Ok(app) => app.config.gemini_api_key.trim().to_string(),
        Err(_) => String::new(),
    };
    if api_key.is_empty() {
        publish_owned(&stop, || {
            super::state::publish_error(
                TranslationGummyConnectionState::Error,
                "missing_api_key".to_string(),
                false,
            )
        });
        return;
    }

    let mut lifecycle = new_lifecycle();
    let epoch = Instant::now();
    lifecycle.reduce(0, LiveLifecycleEvent::Start);
    let mut reconnecting = false;
    while !stop.load(Ordering::SeqCst) {
        if !publish_owned(&stop, || {
            super::state::publish_connection(
                if reconnecting {
                    TranslationGummyConnectionState::Reconnecting
                } else {
                    TranslationGummyConnectionState::Connecting
                },
                true,
                None,
            )
        }) {
            break;
        }

        let result = run_single_session(
            hwnd,
            &api_key,
            &settings,
            stop.clone(),
            playback_ids.clone(),
            &mut lifecycle,
            epoch,
        );
        if stop.load(Ordering::SeqCst) {
            break;
        }

        match result {
            Ok(()) => break,
            Err(error) => {
                stop_playback(&playback_ids);
                let now = epoch.elapsed().as_millis() as u64;
                let generation = lifecycle.state().generation;
                let effects = lifecycle.reduce(
                    now,
                    LiveLifecycleEvent::TransportFailure {
                        generation,
                        retryable: retryable(&error),
                    },
                );
                let delay = effects.iter().find_map(|effect| match effect {
                    LiveLifecycleEffect::ScheduleReconnect { delay_ms, .. } => Some(*delay_ms),
                    _ => None,
                });
                let Some(delay) = delay else {
                    publish_owned(&stop, || {
                        super::state::publish_error(
                            TranslationGummyConnectionState::Error,
                            "connection_lost".to_string(),
                            false,
                        )
                    });
                    break;
                };
                publish_owned(&stop, || {
                    super::state::publish_connection(
                        TranslationGummyConnectionState::Reconnecting,
                        true,
                        None,
                    )
                });
                let deadline = Instant::now() + Duration::from_millis(delay);
                while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
                    std::thread::sleep(
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(50)),
                    );
                }
                lifecycle.reduce(
                    epoch.elapsed().as_millis() as u64,
                    LiveLifecycleEvent::Start,
                );
                reconnecting = true;
            }
        }
    }
}

fn new_lifecycle() -> LiveSessionLifecycle {
    let mut policy = LiveLifecyclePolicy::continuous();
    policy.max_reconnect_attempts = Some(6);
    policy.rotate_after_ms = None;
    policy.server_idle_timeout_ms = None;
    LiveSessionLifecycle::new(policy, LiveBackoffPolicy::default())
}

fn run_single_session(
    hwnd: isize,
    api_key: &str,
    settings: &TranslationGummySettings,
    stop: Arc<std::sync::atomic::AtomicBool>,
    playback_ids: Arc<Mutex<Vec<u64>>>,
    lifecycle: &mut LiveSessionLifecycle,
    epoch: Instant,
) -> anyhow::Result<()> {
    let mut socket = connect_websocket(api_key)?;
    if stop.load(Ordering::SeqCst) {
        return Ok(());
    }
    let generation = lifecycle.state().generation;
    lifecycle.reduce(
        epoch.elapsed().as_millis() as u64,
        LiveLifecycleEvent::SocketOpened { generation },
    );
    send_setup(&mut socket, settings)?;
    set_socket_short_timeout(&mut socket)?;
    let setup_frames = wait_for_setup(&mut socket, stop.clone())?;
    if stop.load(Ordering::SeqCst) {
        return Ok(());
    }
    lifecycle.reduce(
        epoch.elapsed().as_millis() as u64,
        LiveLifecycleEvent::Frame(LiveLifecycleFrame {
            generation,
            setup_complete: true,
            ..Default::default()
        }),
    );
    set_socket_nonblocking(&mut socket)?;

    if !publish_owned(&stop, || {
        super::state::insert_session_separator();
        super::state::publish_connection(TranslationGummyConnectionState::Ready, true, None);
        super::state::publish_audio_level(0.0);
    }) {
        return Ok(());
    }
    request_persist();

    let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
    let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stream = start_mic_capture(buffer.clone(), stop.clone(), pause)
        .map_err(|_| SessionFailure::Audio)?;
    let _keep_stream_alive = stream;
    if stop.load(Ordering::SeqCst) {
        return Ok(());
    }

    let mut playback = PlaybackBridge::new(hwnd, stop.clone(), playback_ids)?;
    if stop.load(Ordering::SeqCst) {
        return Ok(());
    }
    for setup_frame in setup_frames {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        handle_update(
            &setup_frame,
            hwnd,
            &mut playback,
            &stop,
            lifecycle,
            epoch.elapsed().as_millis() as u64,
        )?;
    }
    let mut pending_audio: Vec<i16> = Vec::new();
    let mut input_turn = LocalInputTurnState::new();

    while !stop.load(Ordering::SeqCst) {
        flush_audio(
            &mut socket,
            &buffer,
            &mut pending_audio,
            &mut input_turn,
            &stop,
        )?;

        match socket.read() {
            Ok(Message::Text(msg)) => {
                handle_update(
                    &msg,
                    hwnd,
                    &mut playback,
                    &stop,
                    lifecycle,
                    epoch.elapsed().as_millis() as u64,
                )?;
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    handle_update(
                        &text,
                        hwnd,
                        &mut playback,
                        &stop,
                        lifecycle,
                        epoch.elapsed().as_millis() as u64,
                    )?;
                }
            }
            Ok(Message::Close(frame)) => {
                playback.end();
                return Err(SessionFailure::Close(
                    frame.map(|f| u16::from(f.code)).unwrap_or(1006),
                )
                .into());
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
    Ok(())
}

pub fn next_transcript_id() -> u64 {
    TRANSCRIPT_COUNTER.fetch_add(1, Ordering::SeqCst)
}
