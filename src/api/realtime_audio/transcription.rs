//! Main transcription loop for realtime audio

pub(crate) mod dedicated;
#[cfg(test)]
mod live_tests;
mod main_loop;
mod reconnect;
mod restart_policy;

use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;

use crate::APP;
use crate::api::gemini_live::ready_session::{ConnectedLiveSocket, OpenOptions, ReadyLiveSession};
use crate::model_config::{
    normalize_realtime_transcription_model_id, realtime_transcription_api_model,
    realtime_transcription_live_protocol,
};
use crate::overlay::realtime_webview::SELECTED_APP_PID;

use super::REALTIME_RMS;
use super::capture::{
    start_device_loopback_capture_resilient, start_mic_capture_resilient, start_per_app_capture,
};
use super::state::SharedRealtimeState;
use super::translation::run_translation_loop;
use restart_policy::{DeviceReconnectBackoff, should_spawn_text_translation};

#[derive(Clone, Debug)]
pub struct RealtimeSessionPlan {
    pub audio_source: String,
    pub target_language: String,
    pub has_translation: bool,
}

fn open_ready_session(
    api_key: &str,
    model: &str,
    vocabulary: &[String],
    resumption_handle: Option<&str>,
    cancelled: impl Fn() -> bool,
) -> Result<ReadyLiveSession> {
    let setup = build_realtime_transcription_setup(model, vocabulary, resumption_handle);
    ConnectedLiveSocket::connect_cancellable(api_key, &cancelled)?.activate_with(
        setup,
        OpenOptions {
            setup_timeout: Duration::from_secs(30),
            ..OpenOptions::default()
        },
        cancelled,
    )
}

/// Capture starts before the remote session: speech during connection setup is
/// buffered and replayed, an unavailable device fails before any session is
/// opened, and an app-selection wait never holds an idle Live session open.
/// `None` from either step means the engine was cancelled or superseded.
fn start_capture_then_session<C, S>(
    start_capture: impl FnOnce() -> Result<Option<C>>,
    open_session: impl FnOnce() -> Result<Option<S>>,
) -> Result<Option<(C, S)>> {
    let Some(capture) = start_capture()? else {
        return Ok(None);
    };
    Ok(open_session()?.map(|session| (capture, session)))
}

fn build_realtime_transcription_setup(
    model: &str,
    vocabulary: &[String],
    resumption_handle: Option<&str>,
) -> serde_json::Value {
    crate::api::gemini_transcribe::build_live_setup(model, vocabulary, resumption_handle)
}

fn reconnect_on_no_results_for_protocol(protocol: Option<&str>) -> bool {
    protocol != Some("live-transcribe")
}

fn setup_cancelled(stop_signal: &Arc<AtomicBool>) -> bool {
    stop_signal.load(Ordering::Relaxed)
        || crate::overlay::realtime_webview::TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
        || crate::overlay::realtime_webview::AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
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
    plan: RealtimeSessionPlan,
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
            plan,
            stop_signal,
            overlay_send,
            translation_send,
            state,
            session_id,
        );
    });
}

fn should_run_text_translation_loop(
    has_translation: bool,
    translation_hwnd: Option<HWND>,
    trans_model: &str,
) -> bool {
    translation_hwnd.is_some()
        && has_translation
        && !crate::model_config::is_gemini_live_s2s_model_id(trans_model)
}

fn spawn_text_translation_loop(
    target_language: String,
    stop_signal: Arc<AtomicBool>,
    translation_send: crate::win_types::SendHwnd,
    state: SharedRealtimeState,
    trans_model: &str,
    session_id: u64,
) -> std::thread::JoinHandle<()> {
    crate::log_info!(
        "[RealtimeTranslate] spawn text translation loop transcription_model={}",
        trans_model
    );
    std::thread::spawn(move || {
        run_translation_loop(
            target_language,
            stop_signal,
            translation_send,
            state,
            session_id,
        );
    })
}

fn transcription_thread_entry(
    plan: RealtimeSessionPlan,
    stop_signal: Arc<AtomicBool>,
    overlay_send: crate::win_types::SendHwnd,
    translation_send: Option<crate::win_types::SendHwnd>,
    state: SharedRealtimeState,
    session_id: u64,
) {
    let hwnd_overlay = overlay_send.0;
    let hwnd_translation = translation_send.map(|h| h.0);

    use crate::overlay::realtime_webview::state::{
        TRANSCRIPTION_ENGINE_REVISION, TRANSCRIPTION_TRANSITION, publish_if_current,
        publish_if_engine_current, publish_if_session,
    };
    use crate::overlay::realtime_webview::{
        AUDIO_SOURCE_CHANGE, LANGUAGE_CHANGE, NEW_AUDIO_SOURCE, TRANSCRIPTION_MODEL_CHANGE,
    };

    let mut audio_source = plan.audio_source;
    let mut freeze_on_next_restart = false;
    let mut text_translation_loop: Option<std::thread::JoinHandle<()>> = None;
    let mut device_reconnect = DeviceReconnectBackoff::default();

    loop {
        let transition = TRANSCRIPTION_TRANSITION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut trans_model = String::new();
        let mut engine_revision = 0;
        if !publish_if_current(session_id, &stop_signal, || {
            AUDIO_SOURCE_CHANGE.store(false, Ordering::SeqCst);
            TRANSCRIPTION_MODEL_CHANGE.store(false, Ordering::SeqCst);
            LANGUAGE_CHANGE.store(false, Ordering::SeqCst);
            super::DEVICE_RECONNECT_REQUESTED.store(false, Ordering::SeqCst);
            if freeze_on_next_restart {
                if let Ok(mut s) = state.lock() {
                    s.freeze_current_transcript();
                }
                freeze_on_next_restart = false;
            }
            REALTIME_RMS.store(0, Ordering::SeqCst);
            let app = APP.lock().unwrap();
            trans_model =
                normalize_realtime_transcription_model_id(&app.config.realtime_transcription_model);
            audio_source = app.config.realtime_audio_source.clone();
            engine_revision = TRANSCRIPTION_ENGINE_REVISION
                .fetch_add(1, Ordering::SeqCst)
                .wrapping_add(1);
        }) {
            break;
        }

        if !publish_if_engine_current(session_id, &stop_signal, engine_revision, || {
            let wants_text_translation = should_run_text_translation_loop(
                plan.has_translation,
                hwnd_translation,
                &trans_model,
            );
            if should_spawn_text_translation(wants_text_translation, text_translation_loop.as_ref())
            {
                if let Some(t_send) = translation_send {
                    text_translation_loop = Some(spawn_text_translation_loop(
                        plan.target_language.clone(),
                        stop_signal.clone(),
                        t_send,
                        state.clone(),
                        &trans_model,
                        session_id,
                    ));
                }
            } else if !wants_text_translation
                && crate::model_config::is_gemini_live_s2s_model_id(&trans_model)
                && hwnd_translation.is_some()
            {
                crate::log_info!(
                    "[RealtimeTranslate] skip text translation loop because direct speech model owns target output transcription_model={}",
                    trans_model
                );
            }

            // Update state with selected method immediately (before the session connects)
            if let Ok(mut s) = state.lock() {
                if crate::model_config::is_gemini_live_s2s_model_id(&trans_model) {
                    s.set_transcription_method(super::state::TranscriptionMethod::GeminiLiveS2s);
                } else {
                    s.set_transcription_method(super::state::TranscriptionMethod::GeminiLive);
                }
            }
        }) {
            continue;
        }
        drop(transition);

        let engine_started = Instant::now();
        let result = if crate::model_config::is_gemini_live_s2s_model_id(&trans_model) {
            super::s2s::run_gemini_live_s2s(
                audio_source.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                hwnd_translation,
                state.clone(),
                session_id,
                engine_revision,
            )
        } else {
            run_realtime_transcription(
                audio_source.clone(),
                stop_signal.clone(),
                hwnd_overlay,
                hwnd_translation,
                state.clone(),
                session_id,
                engine_revision,
                &trans_model,
            )
        };

        let mut reconnect_requested = false;
        let mut restart_source = false;
        let mut restart_model = false;
        let mut restart_language = false;
        if !publish_if_session(session_id, || {
            reconnect_requested = super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst);
            restart_source = AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst);
            restart_model = TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst);
            restart_language = LANGUAGE_CHANGE.load(Ordering::SeqCst);
            if let Err(e) = result
                && !restart_source
                && !restart_model
                && !reconnect_requested
                && !crate::api::gemini_live::transport::is_recoverable_anyhow_socket_error(&e)
                && !stop_signal.load(Ordering::SeqCst)
            {
                eprintln!("Realtime transcription error: {}", e);
                crate::overlay::auto_copy_badge::enqueue_notification_with_duration(
                    format!("Realtime transcription error: {e}"),
                    String::new(),
                    crate::overlay::auto_copy_badge::NotificationType::Error,
                    Some(10_000),
                );
            }
            if restart_source
                && let Ok(new_source) = NEW_AUDIO_SOURCE.lock()
                && !new_source.is_empty()
            {
                audio_source.clone_from(&new_source);
            }
            if restart_source || restart_model || restart_language {
                freeze_on_next_restart = true;
            }
            if restart_source || restart_model || restart_language || reconnect_requested {
                stop_signal.store(false, Ordering::SeqCst);
            }
        }) {
            break;
        }

        if reconnect_requested {
            if let Some(delay) = device_reconnect.next_delay(engine_started.elapsed()) {
                std::thread::sleep(delay);
                continue;
            }
            crate::overlay::auto_copy_badge::enqueue_notification_with_duration(
                "Realtime audio device is unavailable. Check the selected audio source."
                    .to_string(),
                String::new(),
                crate::overlay::auto_copy_badge::NotificationType::Error,
                Some(10_000),
            );
            break;
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
}

#[expect(
    clippy::too_many_arguments,
    reason = "the session carries model and engine identity"
)]
fn run_realtime_transcription(
    audio_source: String,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    _translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
    session_id: u64,
    engine_revision: u64,
    selected_model_id: &str,
) -> Result<()> {
    let gemini_api_key = {
        let app = APP.lock().unwrap();
        app.config.gemini_api_key.clone()
    };
    let gemini_live_model = realtime_transcription_api_model(selected_model_id);
    let live_protocol = realtime_transcription_live_protocol(selected_model_id);
    let reconnect_on_no_results = reconnect_on_no_results_for_protocol(live_protocol);
    let uses_interim_transcripts = live_protocol == Some("live-transcribe");

    if gemini_api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }

    // Set transcription method to GeminiLive (uses delimiter-based segmentation)
    if !crate::overlay::realtime_webview::state::publish_if_engine_current(
        session_id,
        &stop_signal,
        engine_revision,
        || {
            if let Ok(mut s) = state.lock() {
                s.set_transcription_method(if uses_interim_transcripts {
                    super::state::TranscriptionMethod::GeminiTranscribe
                } else {
                    super::state::TranscriptionMethod::GeminiLive
                });
            }
        },
    ) {
        return Ok(());
    }

    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));

    use crate::overlay::realtime_webview::REALTIME_TTS_ENABLED;
    let tts_enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let using_device_loopback = audio_source == "device" && !tts_enabled;
    let mut using_per_app_capture = false;
    let engine_cancelled = || {
        setup_cancelled(&stop_signal)
            || crate::overlay::realtime_webview::state::TRANSCRIPTION_ENGINE_REVISION
                .load(Ordering::SeqCst)
                != engine_revision
    };

    let start_capture = || -> Result<Option<super::capture::CaptureStream>> {
        let selected_pid = if audio_source == "device" && tts_enabled && selected_pid == 0 {
            crate::log_info!(
                "[RealtimeGeminiLiveHealth] app-selection-required source=device tts_enabled=true"
            );
            crate::overlay::realtime_webview::app_selection::show_audio_app_selector_for_session(
                session_id,
                engine_revision,
                stop_signal.clone(),
                None,
            );
            wait_for_selected_audio_app(&stop_signal)
        } else if selected_pid > 0 {
            Some(selected_pid)
        } else {
            None
        };
        using_per_app_capture = audio_source == "device" && tts_enabled && selected_pid.is_some();
        let dummy_pause = Arc::new(AtomicBool::new(false));
        let capture_owner = super::capture::RealtimeCaptureOwner::new(
            session_id,
            engine_revision,
            stop_signal.clone(),
        );
        if using_per_app_capture {
            return Ok(Some(
                start_per_app_capture(
                    selected_pid.unwrap_or_default(),
                    audio_buffer.clone(),
                    stop_signal.clone(),
                    dummy_pause,
                    Some(capture_owner),
                )?
                .into(),
            ));
        }
        if using_device_loopback {
            return start_device_loopback_capture_resilient(
                audio_buffer.clone(),
                stop_signal.clone(),
                dummy_pause,
                Some(capture_owner),
            )
            .map(Some);
        }
        if audio_source == "device" && tts_enabled {
            crate::log_info!(
                "[RealtimeGeminiLiveHealth] no-capture reason=app-selection-cancelled source=device tts_enabled=true"
            );
            return Ok(None);
        }
        start_mic_capture_resilient(
            audio_buffer.clone(),
            stop_signal.clone(),
            dummy_pause,
            Some(capture_owner),
        )
        .map(Some)
    };
    let open_session = || -> Result<Option<ReadyLiveSession>> {
        let (_, vocabulary) = dedicated::vocabulary_snapshot();
        match open_ready_session(
            &gemini_api_key,
            &gemini_live_model,
            &vocabulary,
            None,
            engine_cancelled,
        ) {
            Ok(session) => Ok(Some(session)),
            Err(_) if engine_cancelled() => Ok(None),
            Err(error) => Err(error),
        }
    };
    let Some((_stream, session)) = start_capture_then_session(start_capture, open_session)? else {
        return Ok(());
    };

    if !crate::overlay::realtime_webview::state::publish_if_engine_current(
        session_id,
        &stop_signal,
        engine_revision,
        || {},
    ) {
        return Ok(());
    }

    // Start translation thread if needed
    // NOTE: Translation thread is now spawned in `start_realtime_transcription`
    // to ensure it runs independent of the transcription model.

    // Main loop
    let capture_label = if using_per_app_capture {
        "per-app"
    } else if using_device_loopback {
        "device"
    } else {
        "mic"
    };
    let loop_result = main_loop::run_main_loop(main_loop::RealtimeMainLoop {
        session,
        audio_buffer,
        stop_signal,
        overlay_hwnd,
        state,
        session_id,
        engine_revision,
        gemini_live_model: &gemini_live_model,
        gemini_api_key: &gemini_api_key,
        capture_label,
        reconnect_on_no_results,
        uses_interim_transcripts,
        #[cfg(test)]
        observations: None,
    });

    drop(_stream);
    loop_result
}

#[cfg(test)]
#[path = "transcription/setup_tests.rs"]
mod tests;
