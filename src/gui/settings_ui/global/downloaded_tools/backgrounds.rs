use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::overlay::screen_record::bg_download;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::utils::{format_size, removal_in_progress, start_removal, tool_card};

static BADGE_MONITOR_ACTIVE: AtomicBool = AtomicBool::new(false);
const REMOVE_BACKGROUNDS: &str = "downloaded-tools:remove-backgrounds";

pub(super) fn render_background_downloads_section(ui: &mut egui::Ui, text: &LocaleText) {
    let summary = bg_download::downloadable_background_summary();
    let theme = AppTheme::from_ui(ui);

    tool_card(ui, |ui| {
        ui.heading(text.auxiliary.managed_tools.tool_downloadable_backgrounds);
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(
                text.auxiliary
                    .managed_tools
                    .tool_desc_downloadable_backgrounds,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if removal_in_progress(REMOVE_BACKGROUNDS) {
                    ui.spinner();
                    ui.label(text.auxiliary.managed_tools.tool_status_removing);
                    return;
                }
                if summary.total_count == 0 {
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                    return;
                }

                if summary.downloading_count > 0 {
                    ui.spinner();
                    ui.label(
                        text.auxiliary
                            .managed_tools
                            .tool_bg_downloading_fmt
                            .replace("{}", &summary.downloading_count.to_string()),
                    );
                    return;
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
                        start_background_removal(text);
                    }
                } else if ui
                    .button(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_bg_action_delete_all)
                            .color(theme.danger_text()),
                    )
                    .clicked()
                {
                    start_background_removal(text);
                }
            });
        });

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count_text = text
                    .auxiliary
                    .managed_tools
                    .tool_bg_downloaded_count_fmt
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
    });
}

fn start_background_removal(text: &LocaleText) {
    start_removal(
        REMOVE_BACKGROUNDS,
        text.auxiliary
            .managed_tools
            .tool_downloadable_backgrounds
            .to_string(),
        || {
            let _recorder = crate::overlay::screen_record::stop_for_component_removal()?;
            bg_download::delete_all_downloaded()
                .map(drop)
                .map_err(anyhow::Error::msg)?;
            Ok(())
        },
        || {},
    );
}

fn start_missing_with_badge(text: &LocaleText) {
    if bg_download::start_download_all_missing() == 0
        || BADGE_MONITOR_ACTIVE.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let name = text
        .auxiliary
        .managed_tools
        .tool_downloadable_backgrounds
        .to_string();
    std::thread::spawn(move || {
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&name);
        loop {
            let summary = bg_download::downloadable_background_summary();
            badge.report(summary.downloaded_count as u64, summary.total_count as u64);
            if summary.downloading_count == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        BADGE_MONITOR_ACTIVE.store(false, Ordering::Release);
    });
}
