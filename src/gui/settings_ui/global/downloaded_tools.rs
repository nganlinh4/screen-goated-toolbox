use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::DownloadManager;
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::time::{Duration, Instant};

mod ai_runtime;
mod backgrounds;
mod cleanup;
mod computer_control;
mod mcp;
mod model_card;
mod model_sections;
mod pointer_packs;
mod recorder;
mod screen_text_detector;
mod three_d_generator;
mod tts_models;
mod utils;
mod video_downloader;
mod web_apps;
mod zipformer;

use self::{
    backgrounds::render_background_downloads_section,
    computer_control::render_computer_control_card,
    mcp::render_mcp_card,
    model_sections::{
        render_kokoro_card, render_parakeet_card, render_qwen3_card, render_supertonic_card,
    },
    pointer_packs::render_pointer_pack_downloads_section,
    recorder::render_recorder_card,
    screen_text_detector::render as render_screen_text_detector_card,
    three_d_generator::render_three_d_generator_card,
    tts_models::{render_magpie_card, render_step_audio_card, render_vieneu_card},
    video_downloader::render_video_downloader_card,
    web_apps::render_web_apps_card,
    zipformer::render_zipformer_section,
};

const SECTION_TIMING_WARN_MS: f64 = 12.0;
const SECTION_TIMING_LOG_INTERVAL: Duration = Duration::from_secs(2);

fn timing_log_state() -> &'static std::sync::Mutex<std::collections::HashMap<&'static str, Instant>>
{
    static STATE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<&'static str, Instant>>,
    > = std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn time_downloaded_tools_section(label: &'static str, render: impl FnOnce()) {
    let started_at = Instant::now();
    render();
    let elapsed = started_at.elapsed();
    if elapsed.as_secs_f64() * 1000.0 < SECTION_TIMING_WARN_MS {
        return;
    }

    let now = Instant::now();
    if let Ok(mut state) = timing_log_state().lock() {
        let should_log = state
            .get(label)
            .is_none_or(|last| now.duration_since(*last) >= SECTION_TIMING_LOG_INTERVAL);
        if !should_log {
            return;
        }
        state.insert(label, now);
    }

    crate::log_info!(
        "[DownloadedToolsPerf] section={} ms={:.1}",
        label,
        elapsed.as_secs_f64() * 1000.0
    );
}

pub fn render_downloaded_tools_modal(
    ctx: &egui::Context,
    _ui: &mut egui::Ui,
    show_modal: &mut bool,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    if *show_modal {
        let mut open = true;
        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);

        // Manual full-viewport scrim behind the (large, resizable) tools window
        // so it reads as the clear focus, matching the modal dialog treatment.
        let screen_rect = ctx.content_rect();
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("downloaded_tools_scrim"),
        ))
        .rect_filled(screen_rect, 0.0, theme.scrim_color());

        egui::Window::new(text.auxiliary.managed_tools.downloaded_tools_title)
            .collapsible(false)
            .resizable(true)
            .title_bar(false)
            .frame(theme.dialog_frame())
            .default_width(1100.0)
            .default_height(540.0)
            .min_width(900.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(900.0);

                if crate::gui::widgets::dialog_header(
                    ui,
                    &theme,
                    text.auxiliary.managed_tools.downloaded_tools_title,
                    None,
                    |ui| {
                        if ui
                            .add_enabled(
                                !cleanup::is_active(),
                                egui::Button::new(
                                    egui::RichText::new(
                                        text.auxiliary.managed_tools.downloaded_tools_clean_all,
                                    )
                                    .color(theme.danger_text()),
                                ),
                            )
                            .clicked()
                        {
                            cleanup::request_confirmation();
                        }
                    },
                ) {
                    open = false;
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(8.0);

                        ui.columns(2, |columns| {
                            columns[0].vertical(|ui| {
                                time_downloaded_tools_section("parakeet-card", || {
                                    render_parakeet_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("qwen3-card", || {
                                    render_qwen3_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("backgrounds", || {
                                    render_background_downloads_section(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("pointer-packs", || {
                                    render_pointer_pack_downloads_section(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("three-d-generator", || {
                                    render_three_d_generator_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("mcp-integrations", || {
                                    render_mcp_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("computer-control", || {
                                    render_computer_control_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("screen-text-detector", || {
                                    render_screen_text_detector_card(ui, text)
                                });
                            });

                            columns[1].vertical(|ui| {
                                time_downloaded_tools_section("video-downloader", || {
                                    render_video_downloader_card(ui, download_manager, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("web-app-interfaces", || {
                                    render_web_apps_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("screen-recorder", || {
                                    render_recorder_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("zipformer", || {
                                    render_zipformer_section(ui, download_manager, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("kokoro", || {
                                    render_kokoro_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("supertonic-tts", || {
                                    render_supertonic_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("step-audio-tts", || {
                                    render_step_audio_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("magpie-tts", || {
                                    render_magpie_card(ui, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("vieneu-tts", || {
                                    render_vieneu_card(ui, text)
                                });
                            });
                        });
                    });
            });

        *show_modal = open;
    }
    cleanup::render(ctx, text, download_manager);
}
