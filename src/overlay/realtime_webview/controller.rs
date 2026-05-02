use std::sync::atomic::Ordering;

use crate::APP;
use crate::overlay::window_selector::{self, SelectorOwner};

use super::app_selection::show_audio_app_selector_overlay;
use super::state::*;

#[derive(Clone, Debug)]
pub struct RealtimeSessionConfig {
    pub audio_source: String,
    pub target_language: String,
    pub translation_model: String,
    pub transcription_model: String,
    pub transcription_language: String,
    pub font_size: u32,
}

pub fn load_session_config() -> RealtimeSessionConfig {
    let app = APP.lock().unwrap();
    RealtimeSessionConfig {
        audio_source: normalize_audio_source(&app.config.realtime_audio_source),
        target_language: app.config.realtime_target_language.clone(),
        translation_model: app.config.realtime_translation_model.clone(),
        transcription_model: crate::model_config::normalize_realtime_transcription_model_id(
            &app.config.realtime_transcription_model,
        ),
        transcription_language: normalize_transcription_language(
            &app.config.realtime_transcription_language,
        ),
        font_size: app.config.realtime_font_size,
    }
}

pub fn normalize_audio_source(source: &str) -> String {
    if source.is_empty() {
        "device".to_string()
    } else {
        source.to_string()
    }
}

pub fn normalize_transcription_language(language: &str) -> String {
    if language == "all" || language.is_empty() {
        "en".to_string()
    } else {
        language.to_string()
    }
}

pub fn reset_runtime_for_new_session() {
    REALTIME_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    unsafe {
        IS_ACTIVE = true;
    }
    REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
    REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
    MIC_VISIBLE.store(true, Ordering::SeqCst);
    TRANS_VISIBLE.store(true, Ordering::SeqCst);
    AUDIO_SOURCE_CHANGE.store(false, Ordering::SeqCst);
    LANGUAGE_CHANGE.store(false, Ordering::SeqCst);
    TRANSLATION_MODEL_CHANGE.store(false, Ordering::SeqCst);
    TRANSCRIPTION_MODEL_CHANGE.store(false, Ordering::SeqCst);
    LAST_SPOKEN_LENGTH.store(0, Ordering::SeqCst);
    REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
    REALTIME_S2S_AUDIO_BACKLOG.store(0, Ordering::SeqCst);
    SELECTED_APP_PID.store(0, Ordering::SeqCst);
    if let Ok(mut name) = SELECTED_APP_NAME.lock() {
        name.clear();
    }
    if let Ok(mut queue) = COMMITTED_TRANSLATION_QUEUE.lock() {
        queue.clear();
    }
    if let Ok(mut state) = REALTIME_STATE.lock() {
        *state = crate::api::realtime_audio::RealtimeState::new();
    }
}

pub fn apply_session_config(config: &RealtimeSessionConfig) {
    if let Ok(mut source) = NEW_AUDIO_SOURCE.lock() {
        *source = config.audio_source.clone();
    }
    if let Ok(mut lang) = NEW_TARGET_LANGUAGE.lock() {
        *lang = config.target_language.clone();
    }
    if let Ok(mut model) = NEW_TRANSLATION_MODEL.lock() {
        *model = config.translation_model.clone();
    }
    if let Ok(mut model) = NEW_TRANSCRIPTION_MODEL.lock() {
        *model = config.transcription_model.clone();
    }
    LANGUAGE_CHANGE.store(!config.target_language.is_empty(), Ordering::SeqCst);
}

pub fn set_audio_source(source: &str) {
    let source = normalize_audio_source(source);
    if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
        *new_source = source.clone();
    }

    let is_s2s = load_session_config().transcription_model == "gemini-live-s2s";
    if source == "mic" {
        clear_selected_app();
    } else if is_s2s || REALTIME_TTS_ENABLED.load(Ordering::SeqCst) {
        show_audio_app_selector_overlay();
    } else {
        clear_selected_app();
    }

    if let Ok(mut app) = APP.lock() {
        app.config.realtime_audio_source = source;
        crate::config::save_config(&app.config);
    }
    AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
}

pub fn set_target_language(language: &str) {
    if let Ok(mut new_lang) = NEW_TARGET_LANGUAGE.lock() {
        *new_lang = language.to_string();
    }
    if let Ok(mut app) = APP.lock() {
        app.config.realtime_target_language = language.to_string();
        crate::config::save_config(&app.config);
    }
    LANGUAGE_CHANGE.store(true, Ordering::SeqCst);
}

pub fn set_translation_model(model: &str) {
    if let Ok(mut new_model) = NEW_TRANSLATION_MODEL.lock() {
        *new_model = model.to_string();
    }
    if let Ok(mut app) = APP.lock() {
        app.config.realtime_translation_model = model.to_string();
        crate::config::save_config(&app.config);
    }
    TRANSLATION_MODEL_CHANGE.store(true, Ordering::SeqCst);
}

pub fn set_transcription_model(model: &str) {
    let model = crate::model_config::normalize_realtime_transcription_model_id(model);
    if let Ok(mut new_model) = NEW_TRANSCRIPTION_MODEL.lock() {
        *new_model = model.clone();
    }
    if let Ok(mut app) = APP.lock() {
        app.config.realtime_transcription_model = model.clone();
        crate::config::save_config(&app.config);
    }
    TRANSCRIPTION_MODEL_CHANGE.store(true, Ordering::SeqCst);
    if model == "gemini-live-s2s" && load_session_config().audio_source == "device" {
        show_audio_app_selector_overlay();
    }
}

pub fn set_transcription_language(language: &str) {
    let language = normalize_transcription_language(language);
    if let Ok(mut app) = APP.lock() {
        app.config.realtime_transcription_language = language;
        crate::config::save_config(&app.config);
    }
    TRANSCRIPTION_MODEL_CHANGE.store(true, Ordering::SeqCst);
}

pub fn set_font_size(font_size: u32) {
    if let Ok(mut app) = APP.lock() {
        app.config.realtime_font_size = font_size;
        crate::config::save_config(&app.config);
    }
}

pub fn set_visibility(transcription_visible: bool, translation_visible: bool) -> bool {
    MIC_VISIBLE.store(transcription_visible, Ordering::SeqCst);
    TRANS_VISIBLE.store(translation_visible, Ordering::SeqCst);
    if !translation_visible {
        crate::api::tts::TTS_MANAGER.stop();
    }
    !transcription_visible && !translation_visible
}

pub fn set_tts_enabled(requested_enabled: bool) {
    if load_session_config().transcription_model == "gemini-live-s2s" {
        REALTIME_TTS_ENABLED.store(true, Ordering::SeqCst);
        return;
    }

    if !requested_enabled {
        disable_tts(true);
        return;
    }

    let source = load_session_config().audio_source;
    if source == "device" {
        REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
        show_audio_app_selector_overlay();
    } else {
        REALTIME_TTS_ENABLED.store(true, Ordering::SeqCst);
    }
}

pub fn disable_tts(close_selector: bool) {
    REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
    crate::api::tts::TTS_MANAGER.stop();
    if close_selector {
        window_selector::close_selector_for_owner(SelectorOwner::RealtimeAppSelection);
    }
    LAST_SPOKEN_LENGTH.store(0, Ordering::SeqCst);
    if let Ok(mut queue) = COMMITTED_TRANSLATION_QUEUE.lock() {
        queue.clear();
    }
    clear_selected_app();
    if load_session_config().audio_source == "device" {
        if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
            *new_source = "device".to_string();
        }
        AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
    }
}

pub fn set_tts_speed(speed: u32) {
    REALTIME_TTS_SPEED.store(speed.clamp(50, 200), Ordering::SeqCst);
    REALTIME_TTS_AUTO_SPEED.store(false, Ordering::SeqCst);
}

pub fn set_tts_auto_speed(enabled: bool) {
    REALTIME_TTS_AUTO_SPEED.store(enabled, Ordering::SeqCst);
}

pub fn set_tts_volume(volume: u32) {
    CURRENT_TTS_VOLUME.store(volume.min(100), Ordering::Relaxed);
}

pub fn process_committed_translation_for_tts(committed: &str, hwnd_val: isize) {
    if committed.is_empty() || !REALTIME_TTS_ENABLED.load(Ordering::SeqCst) {
        return;
    }

    let source = NEW_AUDIO_SOURCE
        .lock()
        .map(|source| source.clone())
        .unwrap_or_else(|_| "mic".to_string());
    let is_mic_mode = source.is_empty() || source == "mic";
    let has_selected_app = SELECTED_APP_PID.load(Ordering::SeqCst) > 0;
    if !is_mic_mode && !has_selected_app {
        return;
    }

    let old_len = committed.len();
    if LAST_SPOKEN_LENGTH.load(Ordering::SeqCst) == 0 && old_len > 50 {
        let text = committed.trim_end();
        let search_limit = text.len().saturating_sub(1);
        if search_limit > 0
            && let Some(idx) = text[..search_limit].rfind(['.', '?', '!', '\n'])
        {
            LAST_SPOKEN_LENGTH.store(idx + 1, Ordering::SeqCst);
        }
    }

    let last_spoken = LAST_SPOKEN_LENGTH.load(Ordering::SeqCst);
    if old_len <= last_spoken {
        return;
    }

    let safe_last_spoken = clamp_to_char_boundary(committed, last_spoken);
    let new_committed = committed[safe_last_spoken..].to_string();
    if !new_committed.trim().is_empty() {
        if let Ok(mut queue) = COMMITTED_TRANSLATION_QUEUE.lock() {
            queue.push_back(new_committed.clone());
        }
        std::thread::spawn(move || {
            crate::api::tts::TTS_MANAGER.speak_realtime(&new_committed, hwnd_val);
        });
    }
    LAST_SPOKEN_LENGTH.store(old_len, Ordering::SeqCst);
}

pub fn cancel_download() {
    crate::api::realtime_audio::cancel_download_and_revert_to_gemini();
}

pub fn stop_runtime_flags() {
    REALTIME_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    REALTIME_SESSION_STOPPING.store(true, Ordering::SeqCst);
    REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
    crate::api::tts::TTS_MANAGER.stop();
    window_selector::close_selector_for_owner(SelectorOwner::RealtimeAppSelection);
}

fn clear_selected_app() {
    SELECTED_APP_PID.store(0, Ordering::SeqCst);
    if let Ok(mut name) = SELECTED_APP_NAME.lock() {
        name.clear();
    }
}

fn clamp_to_char_boundary(text: &str, index: usize) -> usize {
    let mut clamped = index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}
