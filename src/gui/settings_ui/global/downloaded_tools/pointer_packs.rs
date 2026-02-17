use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::pointer_gallery;
use eframe::egui;

pub(super) fn render_pointer_pack_downloads_section(ui: &mut egui::Ui, text: &LocaleText) {
    let summary = pointer_gallery::downloadable_collection_summary();
    let status_id = egui::Id::new("pointer_pack_tools_status");

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(text.tool_downloadable_pointer_collections).strong());
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

                let has_backup = pointer_gallery::has_original_cursor_backup_file();
                if has_backup && ui.button(text.pointer_restore_original_btn).clicked() {
                    let result = pointer_gallery::restore_original_cursor_from_backup();
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(status_id, result);
                    });
                }

                if summary.downloaded_count == 0 {
                    if ui.button(text.tool_bg_action_download_all).clicked() {
                        let _ = pointer_gallery::start_download_all_collections();
                    }
                } else if summary.downloaded_count < summary.total_count {
                    if ui.button(text.tool_bg_action_download_rest).clicked() {
                        let _ = pointer_gallery::start_download_all_collections();
                    }
                    if ui
                        .button(
                            egui::RichText::new(text.tool_bg_action_delete_downloaded)
                                .color(egui::Color32::RED),
                        )
                        .clicked()
                    {
                        let _ = pointer_gallery::delete_downloaded_collections();
                    }
                } else if ui
                    .button(
                        egui::RichText::new(text.tool_bg_action_delete_all)
                            .color(egui::Color32::RED),
                    )
                    .clicked()
                {
                    let _ = pointer_gallery::delete_downloaded_collections();
                }
            });
        });

        ui.horizontal(|ui| {
            ui.label(text.tool_desc_downloadable_pointer_collections);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count_text = text
                    .tool_pointer_downloaded_count_fmt
                    .replacen("{}", &summary.downloaded_count.to_string(), 1)
                    .replacen("{}", &summary.total_count.to_string(), 1);
                let count_text =
                    format!("{} ({})", count_text, format_size(summary.downloaded_bytes));
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

        if let Some(result) = ui
            .ctx()
            .memory(|mem| mem.data.get_temp::<Result<(), String>>(status_id))
        {
            match result {
                Ok(()) => {
                    ui.label(
                        egui::RichText::new(text.pointer_restore_success)
                            .color(egui::Color32::from_rgb(34, 139, 34)),
                    );
                }
                Err(message) => {
                    ui.label(egui::RichText::new(message).color(egui::Color32::RED));
                }
            }
        }
    });
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    format!("{:.1} MB", mb)
}
