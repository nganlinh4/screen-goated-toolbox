use std::sync::atomic::Ordering;

use crate::APP;
use crate::overlay::window_selector::{self, SelectorOwner};

use super::app_selection::show_audio_app_selector_for_session;
use super::state::*;
use super::target_change::{retry_untranslated_tail, update_target_revisions};

mod renderer_publication;
mod tts_toggle;
use renderer_publication::publish_if_renderer_engine_current;
pub(super) use tts_toggle::set_tts_enabled;

#[derive(Clone, Debug)]
pub struct RealtimeSessionConfig {
    pub audio_source: String,
    pub target_language: String,
    pub translation_model: String,
    pub transcription_model: String,
    pub custom_vocabulary: Vec<String>,
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
        custom_vocabulary: app.config.realtime_custom_vocabulary.clone(),
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
    REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
    stop_read_playback();
    let _publication = REALTIME_SESSION_PUBLICATION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    crate::api::tts::TTS_MANAGER.invalidate_realtime_epoch();
    REALTIME_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    begin_stop_signal_session();
    IS_ACTIVE.store(true, Ordering::SeqCst);
    REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
    MIC_VISIBLE.store(true, Ordering::SeqCst);
    TRANS_VISIBLE.store(true, Ordering::SeqCst);
    AUDIO_SOURCE_CHANGE.store(false, Ordering::SeqCst);
    CLOSE_TTS_MODAL_REQUEST.store(false, Ordering::SeqCst);
    LANGUAGE_CHANGE.store(false, Ordering::SeqCst);
    TRANSLATION_MODEL_CHANGE.store(false, Ordering::SeqCst);
    TRANSCRIPTION_MODEL_CHANGE.store(false, Ordering::SeqCst);
    reset_read_cursor();
    SELECTED_APP_PID.store(0, Ordering::SeqCst);
    if let Ok(mut name) = SELECTED_APP_NAME.lock() {
        name.clear();
    }
    if let Ok(mut state) = REALTIME_STATE.lock() {
        *state = crate::api::realtime_audio::RealtimeState::new();
    }
}

pub fn apply_session_config(config: &RealtimeSessionConfig) {
    reconcile_read_model(&config.transcription_model);
    crate::api::realtime_audio::set_vocabulary(&config.custom_vocabulary.join("\n"));
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

pub fn set_custom_vocabulary(lines: &str) {
    crate::api::realtime_audio::set_vocabulary(lines);
    let vocabulary = crate::api::realtime_audio::vocabulary_snapshot().1;
    persist_overlay_control(move |config| config.realtime_custom_vocabulary = vocabulary);
}

pub fn set_audio_source(source: &str, session_id: u64, generation: u64) {
    let _transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let source = normalize_audio_source(source);
    let (engine, stop, tts_epoch) = {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let stop = current_stop_signal();
        if REALTIME_SESSION_ID.load(Ordering::SeqCst) != session_id
            || stop.load(Ordering::SeqCst)
            || !super::supervisor::is_current_generation(generation)
        {
            return;
        }
        if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
            *new_source = source.clone();
        }
        persist_overlay_config({
            let source = source.clone();
            move |config| config.realtime_audio_source = source
        });
        TRANSLATION_OPTIONS_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSCRIPTION_ENGINE_REVISION.fetch_add(1, Ordering::SeqCst);
        AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
        (
            TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst),
            stop,
            crate::api::tts::TTS_MANAGER.realtime_epoch(),
        )
    };
    super::download::notify();
    let retired = retire_read_playback_if_epoch(tts_epoch).unwrap_or_default();
    drop(_transition);
    notify_read_retired(retired);
    request_overlay_repaint();

    let is_s2s = crate::model_config::is_gemini_live_s2s_model_id(
        &load_session_config().transcription_model,
    );
    if source == "mic" {
        publish_if_renderer_engine_current(
            session_id,
            &stop,
            engine,
            generation,
            clear_selected_app,
        );
    } else if is_s2s || REALTIME_TTS_ENABLED.load(Ordering::SeqCst) {
        show_audio_app_selector_for_session(session_id, engine, stop, Some(generation));
    } else {
        publish_if_renderer_engine_current(
            session_id,
            &stop,
            engine,
            generation,
            clear_selected_app,
        );
    }
}

pub fn set_target_language(language: &str) {
    let _transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Ok(mut new_lang) = NEW_TARGET_LANGUAGE.lock() {
            *new_lang = language.to_string();
        }
        let language = language.to_string();
        let mut direct_speech = false;
        persist_overlay_config(|config| {
            config.realtime_target_language = language;
            direct_speech = crate::model_config::is_gemini_live_s2s_model_id(
                &config.realtime_transcription_model,
            );
        });
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.translation_history.clear();
            retry_untranslated_tail(&mut state);
        }
        update_target_revisions(
            direct_speech,
            &TRANSLATION_OPTIONS_REVISION,
            &TRANSCRIPTION_ENGINE_REVISION,
            &LANGUAGE_CHANGE,
        );
    }
    super::download::notify();
    let retired = retire_read_playback();
    drop(_transition);
    notify_read_retired(retired);
    request_overlay_repaint();
}

/// Commit selector state only while its original session and engine still own it.
pub(crate) fn publish_audio_selection(
    session: u64,
    engine: u64,
    valid: impl FnOnce() -> bool,
    effect: impl FnOnce(),
) -> bool {
    let transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tts_epoch = {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if REALTIME_SESSION_ID.load(Ordering::SeqCst) != session
            || TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst) != engine
            || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst)
            || current_stop_signal().load(Ordering::SeqCst)
            || !valid()
        {
            return false;
        }
        effect();
        TRANSLATION_OPTIONS_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSCRIPTION_ENGINE_REVISION.fetch_add(1, Ordering::SeqCst);
        AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
        crate::api::tts::TTS_MANAGER.realtime_epoch()
    };
    super::download::notify();
    let retired = retire_read_playback_if_epoch(tts_epoch).unwrap_or_default();
    drop(transition);
    notify_read_retired(retired);
    true
}

pub fn set_translation_model(model: &str) {
    {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if crate::model_config::is_gemini_live_s2s_model_id(
            &load_session_config().transcription_model,
        ) {
            return;
        }
        if let Ok(mut new_model) = NEW_TRANSLATION_MODEL.lock() {
            *new_model = model.to_string();
        }
        let model = model.to_string();
        persist_overlay_config(move |config| config.realtime_translation_model = model);
        if let Ok(mut state) = REALTIME_STATE.lock() {
            retry_untranslated_tail(&mut state);
        }
        TRANSLATION_OPTIONS_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSLATION_MODEL_CHANGE.store(true, Ordering::SeqCst);
    }
    request_overlay_repaint();
}

pub fn set_transcription_model(model: &str, session_id: u64, generation: u64) {
    let _transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let model = crate::model_config::normalize_realtime_transcription_model_id(model);
    let is_s2s = crate::model_config::is_gemini_live_s2s_model_id(&model);
    let (read_enabled, engine, stop, tts_epoch) = {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let stop = current_stop_signal();
        if REALTIME_SESSION_ID.load(Ordering::SeqCst) != session_id
            || stop.load(Ordering::SeqCst)
            || !super::supervisor::is_current_generation(generation)
        {
            return;
        }
        let enabled = reconcile_read_model(&model);
        if let Ok(mut new_model) = NEW_TRANSCRIPTION_MODEL.lock() {
            *new_model = model.clone();
        }
        persist_overlay_config({
            let model = model.clone();
            move |config| config.realtime_transcription_model = model
        });
        TRANSLATION_OPTIONS_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSCRIPTION_ENGINE_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSCRIPTION_MODEL_CHANGE.store(true, Ordering::SeqCst);
        (
            enabled,
            TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst),
            stop,
            crate::api::tts::TTS_MANAGER.realtime_epoch(),
        )
    };
    super::download::notify();
    let retired = retire_read_playback_if_epoch(tts_epoch).unwrap_or_default();
    drop(_transition);
    notify_read_retired(retired);
    request_overlay_repaint();
    publish_if_renderer_engine_current(session_id, &stop, engine, generation, || {
        super::parent::update_tts(read_enabled, CURRENT_TTS_SPEED.load(Ordering::Relaxed));
    });
    if load_session_config().audio_source != "device" {
        return;
    }

    if is_s2s {
        show_audio_app_selector_for_session(session_id, engine, stop, Some(generation));
    } else if REALTIME_TTS_ENABLED.load(Ordering::SeqCst) {
        publish_if_renderer_engine_current(
            session_id,
            &stop,
            engine,
            generation,
            clear_selected_app,
        );
        show_audio_app_selector_for_session(session_id, engine, stop, Some(generation));
    }
}

pub(super) fn reconcile_read_model(model: &str) -> bool {
    let forced = crate::model_config::is_gemini_live_s2s_model_id(model);
    let enabled = reconcile_read_enabled(&REALTIME_TTS_ENABLED, forced);
    crate::log_info!(
        "[RealtimeRead] model_reconciled direct_speech={} enabled={}",
        forced,
        enabled
    );
    enabled
}

fn reconcile_read_enabled(enabled: &std::sync::atomic::AtomicBool, forced: bool) -> bool {
    enabled.fetch_or(forced, Ordering::SeqCst) || forced
}

pub fn set_transcription_language(language: &str) {
    let _transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let language = normalize_transcription_language(language);
        persist_overlay_config(move |config| config.realtime_transcription_language = language);
        TRANSLATION_OPTIONS_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSCRIPTION_ENGINE_REVISION.fetch_add(1, Ordering::SeqCst);
        TRANSCRIPTION_MODEL_CHANGE.store(true, Ordering::SeqCst);
    }
    super::download::notify();
    let retired = retire_read_playback();
    drop(_transition);
    notify_read_retired(retired);
    request_overlay_repaint();
}

pub fn set_font_size(font_size: u32) {
    persist_overlay_control(move |config| config.realtime_font_size = font_size);
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

#[derive(Default)]
struct ReadCursor {
    text: String,
    revision: Option<u64>,
}

impl ReadCursor {
    fn update(&mut self, committed: &str, revision: u64) -> (bool, Option<String>) {
        let replaced =
            self.revision.is_some_and(|old| old != revision) || !committed.starts_with(&self.text);
        if replaced {
            self.text.clear();
        }
        let mut start = self.text.len();
        if start == 0
            && committed.chars().count() > 50
            && let Some(end) = previous_sentence_end(committed)
        {
            start = end;
        }
        let segment = &committed[start..];
        self.text = committed.to_owned();
        self.revision = Some(revision);
        (
            replaced,
            (!segment.trim().is_empty()).then(|| segment.to_owned()),
        )
    }
}

static READ_CURSOR: std::sync::Mutex<ReadCursor> = std::sync::Mutex::new(ReadCursor {
    text: String::new(),
    revision: None,
});

pub(crate) fn reset_read_cursor() {
    if let Ok(mut cursor) = READ_CURSOR.lock() {
        *cursor = ReadCursor::default();
    }
}

pub(crate) fn stop_read_playback() {
    let retired = retire_read_playback();
    notify_read_retired(retired);
}

pub(crate) fn retire_read_playback() -> Vec<(isize, u64)> {
    let _cursor = READ_CURSOR
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    crate::api::tts::TTS_MANAGER.retire_realtime()
}

pub(crate) fn retire_read_playback_if_epoch(epoch: u64) -> Option<Vec<(isize, u64)>> {
    let _cursor = READ_CURSOR
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    crate::api::tts::TTS_MANAGER.retire_realtime_if_epoch(epoch)
}

pub(crate) fn notify_read_retired(retired: Vec<(isize, u64)>) {
    crate::api::tts::TTS_MANAGER.notify_retired_playback(retired);
}

pub(crate) fn set_translation_visibility(visible: bool) {
    let _cursor = READ_CURSOR
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    TRANS_VISIBLE.store(visible, Ordering::SeqCst);
    let retired = if !visible {
        crate::api::tts::TTS_MANAGER.retire_realtime()
    } else {
        Vec::new()
    };
    drop(_cursor);
    notify_read_retired(retired);
}

pub fn process_committed_translation_for_tts(committed: &str, revision: u64, hwnd_val: isize) {
    if !REALTIME_TTS_ENABLED.load(Ordering::SeqCst)
        || !TRANS_VISIBLE.load(Ordering::SeqCst)
        || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst)
        || current_stop_signal().load(Ordering::SeqCst)
    {
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

    let Ok(mut cursor) = READ_CURSOR.lock() else {
        return;
    };
    if !REALTIME_TTS_ENABLED.load(Ordering::SeqCst)
        || !TRANS_VISIBLE.load(Ordering::SeqCst)
        || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst)
        || current_stop_signal().load(Ordering::SeqCst)
    {
        return;
    }
    let (replaced, new_committed) = cursor.update(committed, revision);
    let retired = if replaced {
        crate::api::tts::TTS_MANAGER.retire_realtime()
    } else {
        Vec::new()
    };
    if let Some(new_committed) = new_committed {
        let epoch = crate::api::tts::TTS_MANAGER.realtime_epoch();
        crate::api::tts::TTS_MANAGER.speak_realtime(&new_committed, hwnd_val, epoch);
    }
    drop(cursor);
    notify_read_retired(retired);
}

pub(super) fn cancel_download_from_renderer(session: u64, generation: u64) {
    let accepted = crate::api::realtime_audio::cancel_download_and_revert_to_gemini(|| {
        crate::api::realtime_audio::renderer_cancel_is_current(
            session,
            generation,
            REALTIME_SESSION_ID.load(Ordering::SeqCst),
            super::supervisor::current_generation(),
            REALTIME_SESSION_STOPPING.load(Ordering::SeqCst),
        )
    });
    if accepted {
        super::wndproc::post_model_and_download_refresh();
    }
}

pub(super) fn cancel_download_for_stop() {
    crate::api::realtime_audio::cancel_download_and_revert_to_gemini(|| true);
}

pub fn stop_runtime_flags() {
    let selector = {
        let _publication = REALTIME_SESSION_PUBLICATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let selector =
            super::app_selection::owner_id_for_session(REALTIME_SESSION_ID.load(Ordering::SeqCst));
        if !REALTIME_SESSION_STOPPING.swap(true, Ordering::SeqCst) {
            REALTIME_SESSION_ID.fetch_add(1, Ordering::SeqCst);
        }
        set_current_stop_signal_locked(true);
        selector
    };
    stop_read_playback();
    if let Some(token) = selector {
        window_selector::close_selector_for_token(SelectorOwner::RealtimeAppSelection, token);
    }
}

fn clear_selected_app() {
    SELECTED_APP_PID.store(0, Ordering::SeqCst);
    if let Ok(mut name) = SELECTED_APP_NAME.lock() {
        name.clear();
    }
}

fn persist_overlay_control(update: impl FnOnce(&mut crate::config::Config)) {
    persist_overlay_config(update);
    request_overlay_repaint();
}

fn persist_overlay_config(update: impl FnOnce(&mut crate::config::Config)) {
    if let Ok(mut app) = APP.lock() {
        update(&mut app.config);
        crate::config::save_config(&app.config);
    }
}

fn request_overlay_repaint() {
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.request_repaint();
    }
}

fn previous_sentence_end(text: &str) -> Option<usize> {
    text.trim_end()
        .char_indices()
        .rev()
        .skip(1)
        .find(|(_, ch)| matches!(ch, '.' | '?' | '!' | '\n'))
        .map(|(index, ch)| index + ch.len_utf8())
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod read_state_tests;
