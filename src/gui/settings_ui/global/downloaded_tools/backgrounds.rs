use crate::gui::locale::LocaleText;
use crate::overlay::screen_record::bg_download;
use eframe::egui;

pub(super) fn render_background_downloads_section(ui: &mut egui::Ui, text: &LocaleText) {
    let summary = bg_download::downloadable_background_summary();

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_downloadable_backgrounds).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if summary.total_count == 0 {
                    ui.label(
                        egui::RichText::new(text.tool_status_missing).color(egui::Color32::GRAY),
                    );
                    return;
                }

                if summary.downloading_count > 0 {
                    ui.spinner();
                    ui.label(
                        text.tool_bg_downloading_fmt
                            .replace("{}", &summary.downloading_count.to_string()),
                    );
                    return;
                }

                if summary.downloaded_count == 0 {
                    if ui.button(text.tool_bg_action_download_all).clicked() {
                        let _ = bg_download::start_download_all_missing();
                    }
                } else if summary.downloaded_count < summary.total_count {
                    if ui.button(text.tool_bg_action_download_rest).clicked() {
                        let _ = bg_download::start_download_all_missing();
                    }
                    if ui
                        .button(
                            egui::RichText::new(text.tool_bg_action_delete_downloaded)
                                .color(egui::Color32::RED),
                        )
                        .clicked()
                    {
                        let _ = bg_download::delete_all_downloaded();
                    }
                } else if ui
                    .button(
                        egui::RichText::new(text.tool_bg_action_delete_all)
                            .color(egui::Color32::RED),
                    )
                    .clicked()
                {
                    let _ = bg_download::delete_all_downloaded();
                }
            });
        });

        ui.horizontal(|ui| {
            ui.label(text.tool_desc_downloadable_backgrounds);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count_text = text
                    .tool_bg_downloaded_count_fmt
                    .replacen("{}", &summary.downloaded_count.to_string(), 1)
                    .replacen("{}", &summary.total_count.to_string(), 1);
                let color =
                    if summary.total_count > 0 && summary.downloaded_count == summary.total_count {
                        egui::Color32::from_rgb(34, 139, 34)
                    } else if summary.downloaded_count > 0 {
                        egui::Color32::from_rgb(255, 165, 0)
                    } else {
                        egui::Color32::GRAY
                    };
                ui.label(egui::RichText::new(count_text).color(color));
            });
        });
    });
}
