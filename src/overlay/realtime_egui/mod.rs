mod rendering;
mod ui;

use crate::APP;
use crate::api::realtime_audio::{RealtimeState, start_realtime_transcription};
use crate::overlay::realtime_webview::state::*;
use eframe::egui;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

lazy_static::lazy_static! {
    pub static ref MINIMAL_ACTIVE: AtomicBool = AtomicBool::new(false);
    pub static ref MINIMAL_PRESET_IDX: AtomicUsize = AtomicUsize::new(0);
    static ref UI_STATE: Mutex<RealtimeUiState> = Mutex::new(RealtimeUiState::default());
    static ref USER_REQUESTED_CLOSE: AtomicBool = AtomicBool::new(false);
}

pub(super) struct RealtimeUiState {
    pub font_size: f32,
    pub apps_list: Vec<(u32, String)>,
    pub show_transcription: bool,
    pub show_translation: bool,
    pub last_spoken_len: usize,
    pub show_app_picker: bool,
    pub show_tts_panel: bool,
    pub last_committed_len: usize,
    pub prev_window_size: egui::Vec2,
    pub prev_has_content: bool,
    pub committed_segments: Vec<String>,
}

impl Default for RealtimeUiState {
    fn default() -> Self {
        Self {
            font_size: 24.0,
            apps_list: Vec::new(),
            show_transcription: true,
            show_translation: true,
            last_spoken_len: 0,
            show_app_picker: false,
            show_tts_panel: false,
            last_committed_len: 0,
            prev_window_size: egui::Vec2::ZERO,
            prev_has_content: false,
            committed_segments: Vec::new(),
        }
    }
}

pub fn show_realtime_egui_overlay(preset_idx: usize) {
    if MINIMAL_ACTIVE.load(Ordering::SeqCst) || unsafe { IS_ACTIVE } {
        return;
    }

    unsafe {
        IS_ACTIVE = true;
        REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
        MIC_VISIBLE.store(true, Ordering::SeqCst);
        TRANS_VISIBLE.store(true, Ordering::SeqCst);
        AUDIO_SOURCE_CHANGE.store(false, Ordering::SeqCst);
        LANGUAGE_CHANGE.store(false, Ordering::SeqCst);
        TRANSLATION_MODEL_CHANGE.store(false, Ordering::SeqCst);

        {
            let mut state = REALTIME_STATE.lock().unwrap();
            *state = RealtimeState::new();
        }
    }

    LAST_SPOKEN_LENGTH.store(0, Ordering::SeqCst);
    REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
    SELECTED_APP_PID.store(0, Ordering::SeqCst);
    if let Ok(mut name) = SELECTED_APP_NAME.lock() {
        name.clear();
    }
    if let Ok(mut queue) = COMMITTED_TRANSLATION_QUEUE.lock() {
        queue.clear();
    }
    USER_REQUESTED_CLOSE.store(false, Ordering::SeqCst);

    MINIMAL_ACTIVE.store(true, Ordering::SeqCst);
    MINIMAL_PRESET_IDX.store(preset_idx, Ordering::SeqCst);

    let app = APP.lock().unwrap();
    let preset = app.config.presets[preset_idx].clone();
    let font_size = app.config.realtime_font_size as f32;
    let config_language = app.config.realtime_target_language.clone();
    let config_audio_source = app.config.realtime_audio_source.clone();
    drop(app);

    let is_device_saved = config_audio_source == "device";

    if let Ok(mut ui_state) = UI_STATE.lock() {
        ui_state.font_size = font_size;
        ui_state.apps_list.clear();
        ui_state.show_transcription = true;
        ui_state.show_translation = true;
        ui_state.last_spoken_len = 0;
        ui_state.last_committed_len = 0;
        ui_state.show_app_picker = is_device_saved;
        ui_state.show_tts_panel = false;
        ui_state.prev_window_size = egui::Vec2::ZERO;
        ui_state.prev_has_content = false;
        ui_state.committed_segments.clear();
        // Don't lazy load apps here to avoid blocking
    }

    let effective_source = if config_audio_source.is_empty() {
        "device".to_string()
    } else {
        config_audio_source
    };

    if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
        *new_source = effective_source.clone();
    }

    if !config_language.is_empty() {
        if let Ok(mut new_lang) = NEW_TARGET_LANGUAGE.lock() {
            *new_lang = config_language.clone();
        }
        LANGUAGE_CHANGE.store(true, Ordering::SeqCst);
    }

    let mut final_preset = preset.clone();
    final_preset.audio_source = effective_source;

    start_realtime_transcription(
        final_preset,
        REALTIME_STOP_SIGNAL.clone(),
        windows::Win32::Foundation::HWND::default(),
        Some(windows::Win32::Foundation::HWND::default()),
        REALTIME_STATE.clone(),
    );

    if let Ok(guard) = crate::gui::GUI_CONTEXT.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

pub fn render_minimal_overlay(ctx: &egui::Context) {
    if !MINIMAL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    if USER_REQUESTED_CLOSE.load(Ordering::SeqCst) {
        MINIMAL_ACTIVE.store(false, Ordering::SeqCst);
        unsafe {
            IS_ACTIVE = false;
        }
        REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
        crate::api::tts::TTS_MANAGER.stop();
        USER_REQUESTED_CLOSE.store(false, Ordering::SeqCst);
        return;
    }

    let mut ui_state = UI_STATE.lock().unwrap();
    let ui_language = APP
        .lock()
        .map(|a| a.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let title = crate::gui::settings_ui::get_localized_preset_name(
        "preset_realtime_audio_translate",
        &ui_language,
    );

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("minimal_realtime_overlay"),
        egui::ViewportBuilder::default()
            .with_inner_size([700.0, 200.0])
            .with_title(title)
            .with_always_on_top(),
        |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                USER_REQUESTED_CLOSE.store(true, Ordering::SeqCst);
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                ui::render_main_ui(ui, &mut ui_state);
            });
        },
    );
}
