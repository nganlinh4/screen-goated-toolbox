use crate::APP;
use crate::overlay::realtime_webview::controller;
use crate::overlay::realtime_webview::state::*;
use eframe::egui;
use std::sync::atomic::Ordering;

use super::RealtimeUiState;
use super::chrome::{
    render_device_warning, render_download_panel, render_transcription_header,
    render_translation_header, render_tts_panel,
};
use super::rendering::{render_transcript, render_translation};
use super::style::{RealtimeEguiTheme, split_panel_frame};

pub(super) fn render_main_ui(ui: &mut egui::Ui, state: &mut RealtimeUiState) {
    let theme = RealtimeEguiTheme::new(ui.visuals().dark_mode);
    let current_source = NEW_AUDIO_SOURCE
        .lock()
        .map(|s| s.clone())
        .unwrap_or_else(|_| "mic".to_string());
    let is_device_mode = current_source == "device";
    let app_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let tts_enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let ui_language = APP
        .lock()
        .map(|a| a.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let locale = crate::gui::locale::LocaleText::get(&ui_language);

    if is_device_mode && tts_enabled && app_pid == 0 {
        render_device_warning(ui, &theme, &locale);
    }

    render_download_panel(ui, &theme, &locale);

    if state.show_tts_panel && state.show_translation {
        render_tts_panel(ui, &theme, is_device_mode, app_pid, tts_enabled, &locale);
    }

    render_content_area(ui, state, &theme, &locale, is_device_mode, tts_enabled);
}

fn render_content_area(
    ui: &mut egui::Ui,
    state: &mut RealtimeUiState,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
    is_device_mode: bool,
    tts_enabled: bool,
) {
    let state_data = REALTIME_STATE.lock().unwrap();
    let font = egui::FontId::new(state.font_size, egui::FontFamily::Proportional);

    if state.show_translation && TRANS_VISIBLE.load(Ordering::SeqCst) {
        controller::process_committed_translation_for_tts(&state_data.committed_translation, 0);
    }

    let (full_transcript, transcript_committed_pos, committed_translation, uncommitted_translation) = (
        state_data.full_transcript.clone(),
        state_data.transcript_committed_pos,
        state_data.committed_translation.clone(),
        state_data.uncommitted_translation.clone(),
    );
    drop(state_data);

    let available_height = ui.available_height();
    let rect = ui.ctx().input(|i| i.viewport().inner_rect);
    let current_window_size = rect.map(|r| r.size()).unwrap_or(egui::Vec2::ZERO);

    let current_len = committed_translation.len();
    if current_len < state.last_committed_len {
        state.committed_segments.clear();
        state.last_committed_len = 0;
    }

    let committed_grew = current_len > state.last_committed_len;
    if committed_grew {
        let new_segment = committed_translation[state.last_committed_len..].to_string();
        state.committed_segments.push(new_segment);
        state.last_committed_len = current_len;
    } else {
        state.last_committed_len = current_len;
    }

    let window_resized = (current_window_size - state.prev_window_size).length() > 1.0;
    if window_resized {
        state.prev_window_size = current_window_size;
    }

    let has_content = !committed_translation.is_empty() || !uncommitted_translation.is_empty();
    let content_appeared = has_content && !state.prev_has_content;
    if has_content != state.prev_has_content {
        state.prev_has_content = has_content;
    }

    let should_scroll_trans = committed_grew || window_resized || content_appeared;

    if state.show_transcription && state.show_translation {
        render_dual_content(
            ui,
            state,
            theme,
            locale,
            is_device_mode,
            tts_enabled,
            available_height,
            &full_transcript,
            transcript_committed_pos,
            &committed_translation,
            &uncommitted_translation,
            &font,
            should_scroll_trans,
        );
    } else if state.show_transcription {
        split_panel_frame(
            ui,
            theme,
            ui.available_width(),
            available_height.max(96.0),
            |ui| {
                render_transcription_header(ui, state, theme, locale, is_device_mode);
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("trans_full")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        render_transcript(ui, &full_transcript, transcript_committed_pos, &font);
                    });
            },
        );
    } else if state.show_translation {
        split_panel_frame(
            ui,
            theme,
            ui.available_width(),
            available_height.max(96.0),
            |ui| {
                render_translation_header(ui, state, theme, locale, tts_enabled);
                ui.add_space(6.0);
                render_translation_scroll(
                    ui,
                    &committed_translation,
                    &uncommitted_translation,
                    &font,
                    should_scroll_trans,
                );
            },
        );
    }
}

fn render_dual_content(
    ui: &mut egui::Ui,
    state: &mut RealtimeUiState,
    theme: &RealtimeEguiTheme,
    locale: &crate::gui::locale::LocaleText,
    is_device_mode: bool,
    tts_enabled: bool,
    available_height: f32,
    full_transcript: &str,
    transcript_committed_pos: usize,
    committed_translation: &str,
    uncommitted_translation: &str,
    font: &egui::FontId,
    should_scroll_trans: bool,
) {
    let available_width = ui.available_width();
    let col_width = ((available_width - 16.0) / 2.0).max(1.0);
    let panel_height = available_height.max(96.0);

    ui.horizontal(|ui| {
        split_panel_frame(ui, theme, col_width, panel_height, |ui| {
            render_transcription_header(ui, state, theme, locale, is_device_mode);
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("trans_scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    render_transcript(ui, full_transcript, transcript_committed_pos, font);
                });
        });

        ui.add_space(8.0);

        split_panel_frame(ui, theme, col_width, panel_height, |ui| {
            render_translation_header(ui, state, theme, locale, tts_enabled);
            ui.add_space(6.0);
            render_translation_scroll(
                ui,
                committed_translation,
                uncommitted_translation,
                font,
                should_scroll_trans,
            );
        });
    });
}

fn render_translation_scroll(
    ui: &mut egui::Ui,
    committed_translation: &str,
    uncommitted_translation: &str,
    font: &egui::FontId,
    should_scroll: bool,
) {
    egui::ScrollArea::vertical()
        .id_salt("transl_scroll")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            render_translation(ui, committed_translation, uncommitted_translation, font);
            if should_scroll {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }
        });
}
