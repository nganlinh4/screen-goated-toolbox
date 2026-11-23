use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::pointer_gallery;
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::backgrounds::pack_status;
use super::utils::{format_size, start_removal, tool_card};

static BADGE_MONITOR_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(super) fn render_pointer_pack_downloads_section(ui: &mut egui::Ui, text: &LocaleText) {
    let snapshot = pack_status::pointers(ui.ctx());
    let downloading_count = pointer_gallery::downloading_collection_count();
    let status_id = egui::Id::new("pointer_pack_tools_status");
    let theme = AppTheme::from_ui(ui);

    tool_card(ui, |ui| {
        ui.heading(
            text.auxiliary
                .managed_tools
                .tool_downloadable_pointer_collections,
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(
                text.auxiliary
                    .managed_tools
                    .tool_desc_downloadable_pointer_collections,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if downloading_count > 0 {
                    ui.spinner();
                    ui.label(
                        text.auxiliary
                            .managed_tools
                            .tool_bg_downloading_fmt
                            .replace("{}", &downloading_count.to_string()),
                    );
                    return;
                }
                let Some(snapshot) = snapshot else {
                    if pack_status::pointer_unavailable() {
                        ui.label(text.auxiliary.managed_tools.tool_status_unavailable);
                    } else {
                        ui.spinner();
                        ui.label(text.auxiliary.download.download_status_checking);
                    }
                    return;
                };
                let summary = snapshot.summary;
                if summary.total_count == 0 {
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                    return;
                }

                let has_backup = snapshot.backup;
                if has_backup
                    && ui
                        .button(text.auxiliary.managed_tools.pointer_restore_original_btn)
                        .clicked()
                {
                    let result = pointer_gallery::restore_original_cursor_from_backup();
                    pack_status::invalidate_pointers();
                    ui.ctx().memory_mut(|mem| {
                        mem.data.insert_temp(status_id, result);
                    });
                }

                if summary.downloaded_count == 0 {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_bg_action_download_all)
                        .clicked()
                    {
                        start_missing_with_badge(text);
                    }
                } else if summary.downloaded_count < summary.total_count {
                    if ui
                        .button(text.auxiliary.managed_tools.tool_bg_action_download_rest)
                        .clicked()
                    {
                        start_missing_with_badge(text);
                    }
                    if ui
                        .button(
                            egui::RichText::new(
                                text.auxiliary
                                    .managed_tools
                                    .tool_bg_action_delete_downloaded,
                            )
                            .color(theme.danger_text()),
                        )
                        .clicked()
                    {
                        start_pack_removal(text);
                    }
                } else if ui
                    .button(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_bg_action_delete_all)
                            .color(theme.danger_text()),
                    )
                    .clicked()
                {
                    start_pack_removal(text);
                }
            });
        });

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let Some(snapshot) = snapshot else {
                    return;
                };
                let summary = snapshot.summary;
                let count_text = text
                    .auxiliary
                    .managed_tools
                    .tool_pointer_downloaded_count_fmt
                    .replacen("{}", &summary.downloaded_count.to_string(), 1)
                    .replacen("{}", &summary.total_count.to_string(), 1);
                let count_text =
                    format!("{} ({})", count_text, format_size(summary.downloaded_bytes));
                let color =
                    if summary.total_count > 0 && summary.downloaded_count == summary.total_count {
                        theme.success()
                    } else if summary.downloaded_count > 0 {
                        theme.warning()
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
                        egui::RichText::new(text.auxiliary.managed_tools.pointer_restore_success)
                            .color(theme.success()),
                    );
                }
                Err(message) => {
                    ui.label(egui::RichText::new(message).color(theme.danger_text()));
                }
            }
        }
    });
}

fn start_missing_with_badge(text: &LocaleText) {
    pack_status::invalidate_pointers();
    if pointer_gallery::start_download_all_collections() == 0
        || BADGE_MONITOR_ACTIVE.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let name = text
        .auxiliary
        .managed_tools
        .tool_downloadable_pointer_collections
        .to_string();
    std::thread::spawn(move || {
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&name);
        while let Ok(summary) = pointer_gallery::scan_downloaded_collections() {
            badge.report(summary.downloaded_count as u64, summary.total_count as u64);
            if summary.downloading_count == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        BADGE_MONITOR_ACTIVE.store(false, Ordering::Release);
        pack_status::invalidate_pointers();
        pack_status::repaint();
    });
}

/// Runs off the UI thread and reports failures (for example a cursor restore
/// error) instead of silently doing nothing.
fn start_pack_removal(text: &LocaleText) {
    let name = text
        .auxiliary
        .managed_tools
        .tool_downloadable_pointer_collections
        .to_string();
    start_removal(
        "downloaded-tools:pointer-packs",
        name,
        || {
            pointer_gallery::delete_downloaded_collections()
                .map(drop)
                .map_err(anyhow::Error::msg)
        },
        pack_status::invalidate_pointers,
    );
}
