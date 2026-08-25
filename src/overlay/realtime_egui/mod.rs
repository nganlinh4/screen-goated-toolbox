mod chrome;
mod rendering;
mod style;
mod ui;

use crate::APP;
use crate::api::realtime_audio::{RealtimeSessionPlan, start_realtime_transcription};
use crate::overlay::realtime_webview::controller;
use crate::overlay::realtime_webview::state::*;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub static MINIMAL_ACTIVE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));
pub static MINIMAL_STOPPING: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));
static UI_STATE: LazyLock<Mutex<RealtimeUiState>> =
    LazyLock::new(|| Mutex::new(RealtimeUiState::default()));
static USER_REQUESTED_CLOSE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));
static LAST_MINIMAL_STOP: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

pub(super) struct RealtimeUiState {
    pub font_size: f32,
    pub show_transcription: bool,
    pub show_translation: bool,
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
            show_transcription: true,
            show_translation: true,
            show_tts_panel: false,
            last_committed_len: 0,
            prev_window_size: egui::Vec2::ZERO,
            prev_has_content: false,
            committed_segments: Vec::new(),
        }
    }
}

pub fn show_realtime_egui_overlay() {
    if MINIMAL_ACTIVE.load(Ordering::SeqCst)
        || MINIMAL_STOPPING.load(Ordering::SeqCst)
        || unsafe { IS_ACTIVE }
        || REALTIME_SESSION_STOPPING.load(Ordering::SeqCst)
    {
        return;
    }

    controller::reset_runtime_for_new_session();
    MINIMAL_STOPPING.store(false, Ordering::SeqCst);
    USER_REQUESTED_CLOSE.store(false, Ordering::SeqCst);

    MINIMAL_ACTIVE.store(true, Ordering::SeqCst);
    let mut session_config = controller::load_session_config();
    if session_config.target_language.is_empty() {
        session_config.target_language = "English".to_string();
    }
    controller::apply_session_config(&session_config);

    if let Ok(mut ui_state) = UI_STATE.lock() {
        ui_state.font_size = session_config.font_size as f32;
        ui_state.show_transcription = true;
        ui_state.show_translation = true;
        ui_state.last_committed_len = 0;
        ui_state.show_tts_panel = false;
        ui_state.prev_window_size = egui::Vec2::ZERO;
        ui_state.prev_has_content = false;
        ui_state.committed_segments.clear();
    }

    start_realtime_transcription(
        RealtimeSessionPlan {
            audio_source: session_config.audio_source,
            target_language: session_config.target_language,
            has_translation: true,
        },
        current_stop_signal(),
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

pub fn stop_minimal_overlay() {
    if MINIMAL_STOPPING.swap(true, Ordering::SeqCst) {
        return;
    }

    if let Ok(mut stopped_at) = LAST_MINIMAL_STOP.lock() {
        *stopped_at = Some(Instant::now());
    }
    controller::stop_runtime_flags();
    MINIMAL_ACTIVE.store(false, Ordering::SeqCst);
    USER_REQUESTED_CLOSE.store(false, Ordering::SeqCst);
    MINIMAL_STOPPING.store(false, Ordering::SeqCst);
    REALTIME_SESSION_STOPPING.store(false, Ordering::SeqCst);
    unsafe {
        IS_ACTIVE = false;
    }
    if let Ok(guard) = crate::gui::GUI_CONTEXT.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

pub fn recently_stopped_minimal() -> bool {
    LAST_MINIMAL_STOP
        .lock()
        .ok()
        .and_then(|stopped_at| *stopped_at)
        .map(|stopped_at| stopped_at.elapsed() < Duration::from_millis(2000))
        .unwrap_or(false)
}

pub fn render_minimal_overlay(ctx: &egui::Context) {
    if !MINIMAL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    if USER_REQUESTED_CLOSE.load(Ordering::SeqCst) {
        stop_minimal_overlay();
        return;
    }

    let mut ui_state = UI_STATE.lock().unwrap();
    let ui_language = APP
        .lock()
        .map(|a| a.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let title = crate::gui::locale::LocaleText::get(&ui_language)
        .live_translate
        .live_translate_title;

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("minimal_realtime_overlay"),
        egui::ViewportBuilder::default()
            .with_inner_size([760.0, 260.0])
            .with_title(title)
            .with_always_on_top(),
        |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                USER_REQUESTED_CLOSE.store(true, Ordering::SeqCst);
            }

            let is_dark = ctx.style().visuals.dark_mode;
            let panel_fill = if is_dark {
                egui::Color32::from_rgba_premultiplied(28, 27, 31, 242)
            } else {
                egui::Color32::from_rgba_premultiplied(254, 247, 255, 242)
            };
            let border = if is_dark {
                egui::Color32::from_rgba_premultiplied(0, 200, 255, 70)
            } else {
                egui::Color32::from_rgba_premultiplied(0, 200, 255, 45)
            };

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(panel_fill)
                        .inner_margin(egui::Margin::symmetric(
                            crate::gui::theme::space::CARD,
                            crate::gui::theme::space::GAP,
                        ))
                        .stroke(egui::Stroke::new(1.0_f32, border))
                        .corner_radius(egui::CornerRadius::same(12)),
                )
                .show(ctx, |ui| {
                    ui::render_main_ui(ui, &mut ui_state);
                });
        },
    );
}
