//! Session-owned realtime speech toggle.
use super::*;

pub(in crate::overlay::realtime_webview) fn set_tts_enabled(
    requested_enabled: bool,
    session_id: u64,
    generation: u64,
) {
    let (engine, stop) = {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let stop = current_stop_signal();
        if REALTIME_SESSION_ID.load(Ordering::SeqCst) != session_id
            || stop.load(Ordering::SeqCst)
            || !super::super::supervisor::is_current_generation(generation)
        {
            return;
        }
        (TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst), stop)
    };
    crate::log_info!(
        "[RealtimeRead] user_requested enabled={}",
        requested_enabled
    );
    if crate::model_config::is_gemini_live_s2s_model_id(&load_session_config().transcription_model)
    {
        publish_if_renderer_engine_current(session_id, &stop, engine, generation, || {
            REALTIME_TTS_ENABLED.store(true, Ordering::SeqCst)
        });
        return;
    }

    if !requested_enabled {
        disable_tts_for_session(session_id, engine, generation);
        return;
    }

    let source = load_session_config().audio_source;
    if source == "device" {
        publish_if_renderer_engine_current(session_id, &stop, engine, generation, || {
            REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst)
        });
        show_audio_app_selector_for_session(session_id, engine, stop, Some(generation));
    } else {
        publish_if_renderer_engine_current(session_id, &stop, engine, generation, || {
            REALTIME_TTS_ENABLED.store(true, Ordering::SeqCst)
        });
    }
}

fn disable_tts_for_session(session_id: u64, engine: u64, generation: u64) {
    let transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let source_is_device = load_session_config().audio_source == "device";
    let (selector, tts_epoch) = {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if REALTIME_SESSION_ID.load(Ordering::SeqCst) != session_id
            || TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst) != engine
            || current_stop_signal().load(Ordering::SeqCst)
            || !super::super::supervisor::is_current_generation(generation)
        {
            return;
        }
        let selector = super::super::app_selection::retire_owner_for_session_locked(session_id);
        REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
        reset_read_cursor();
        clear_selected_app();
        if source_is_device {
            if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
                *new_source = "device".to_string();
            }
            TRANSLATION_OPTIONS_REVISION.fetch_add(1, Ordering::SeqCst);
            TRANSCRIPTION_ENGINE_REVISION.fetch_add(1, Ordering::SeqCst);
            AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
        }
        (selector, crate::api::tts::TTS_MANAGER.realtime_epoch())
    };
    if source_is_device {
        super::super::download::notify();
    }
    let retired = retire_read_playback_if_epoch(tts_epoch).unwrap_or_default();
    drop(transition);
    notify_read_retired(retired);
    if let Some(token) = selector {
        window_selector::close_selector_for_token(SelectorOwner::RealtimeAppSelection, token);
    }
}
