use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::DownloadManager;
use eframe::egui;
use std::time::{Duration, Instant};

mod ai_runtime;
mod backgrounds;
mod model_sections;
mod pointer_packs;
mod utils;
mod video_downloader;
mod zipformer;

use self::{
    backgrounds::render_background_downloads_section,
    model_sections::{render_parakeet_card, render_qwen3_card},
    pointer_packs::render_pointer_pack_downloads_section,
    video_downloader::render_video_downloader_card,
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
        egui::Window::new(text.downloaded_tools_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(1100.0)
            .default_height(540.0)
            .min_width(900.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(900.0);
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
                            });

                            columns[1].vertical(|ui| {
                                time_downloaded_tools_section("video-downloader", || {
                                    render_video_downloader_card(ui, download_manager, text)
                                });
                                ui.add_space(8.0);
                                time_downloaded_tools_section("zipformer", || {
                                    render_zipformer_section(ui, download_manager, text)
                                });
                            });
                        });
                    });
            });

        *show_modal = open;
    }
}
