use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::DownloadManager;
use eframe::egui;

mod ai_runtime;
mod backgrounds;
mod model_sections;
mod pointer_packs;
mod utils;
mod video_downloader;
mod webview2;
mod zipformer;

use self::{
    backgrounds::render_background_downloads_section,
    model_sections::{render_parakeet_card, render_qwen3_card},
    pointer_packs::render_pointer_pack_downloads_section,
    video_downloader::render_video_downloader_card,
    webview2::render_webview2_section,
    zipformer::render_zipformer_section,
};

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
            .default_height(510.0)
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
                                render_parakeet_card(ui, text);
                                ui.add_space(8.0);
                                render_qwen3_card(ui, text);
                                ui.add_space(8.0);
                                render_background_downloads_section(ui, text);
                                ui.add_space(8.0);
                                render_pointer_pack_downloads_section(ui, text);
                            });

                            columns[1].vertical(|ui| {
                                render_video_downloader_card(ui, download_manager, text);
                                ui.add_space(8.0);
                                render_webview2_section(ui, text);
                                ui.add_space(8.0);
                                render_zipformer_section(ui, download_manager, text);
                            });
                        });
                    });
            });

        *show_modal = open;
    }
}
