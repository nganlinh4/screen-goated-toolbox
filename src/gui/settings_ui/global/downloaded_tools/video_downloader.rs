use crate::component_registry::external_tools::{self, ExternalTool, version_label};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus};
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::sync::{Mutex, OnceLock};

use super::utils::tool_card;

static RECOVERY_FEEDBACK: OnceLock<Mutex<[Option<String>; 3]>> = OnceLock::new();

pub(super) fn render_video_downloader_card(
    ui: &mut egui::Ui,
    download_manager: &mut DownloadManager,
    text: &LocaleText,
) {
    tool_card(ui, |ui| {
        ui.heading(text.auxiliary.managed_tools.tool_video_downloader_card);
        ui.add_space(4.0);

        render_tool(
            ui,
            download_manager,
            text,
            ExternalTool::YtDlp,
            text.auxiliary.managed_tools.tool_ytdlp,
            text.auxiliary.managed_tools.tool_desc_ytdlp,
        );
        ui.separator();
        render_tool(
            ui,
            download_manager,
            text,
            ExternalTool::Ffmpeg,
            text.auxiliary.managed_tools.tool_ffmpeg,
            text.auxiliary.managed_tools.tool_desc_ffmpeg,
        );
        ui.separator();
        render_tool(
            ui,
            download_manager,
            text,
            ExternalTool::Deno,
            text.auxiliary.managed_tools.tool_deno,
            text.auxiliary.managed_tools.tool_desc_deno,
        );
    });
}

fn render_tool(
    ui: &mut egui::Ui,
    download_manager: &DownloadManager,
    text: &LocaleText,
    tool: ExternalTool,
    name: &str,
    description: &str,
) {
    let theme = AppTheme::from_ui(ui);
    let status = tool_status(download_manager, tool);
    let mut install_clicked = false;
    let mut remove_clicked = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(name).strong());
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match &status {
                InstallStatus::Installed => {
                    remove_clicked = ui
                        .button(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .clicked();
                    ui.label(
                        egui::RichText::new(
                            text.auxiliary
                                .managed_tools
                                .tool_status_installed
                                .replace("{}", &localized_version_label(tool, text)),
                        )
                        .color(theme.success()),
                    );
                }
                InstallStatus::Downloading(progress) => {
                    ui.spinner();
                    ui.label(format!("{:.0}%", progress * 100.0));
                }
                InstallStatus::Extracting => {
                    ui.spinner();
                    ui.label(text.auxiliary.download.download_status_extracting);
                }
                InstallStatus::Checking => {
                    ui.spinner();
                }
                InstallStatus::Missing => {
                    install_clicked = ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .clicked();
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_missing)
                            .color(egui::Color32::GRAY),
                    );
                }
                InstallStatus::Error(error) => {
                    remove_clicked = ui
                        .button(
                            egui::RichText::new(text.auxiliary.managed_tools.tool_action_delete)
                                .color(theme.danger_text()),
                        )
                        .on_hover_text(error)
                        .clicked();
                    install_clicked = ui
                        .button(text.auxiliary.managed_tools.tool_action_download)
                        .on_hover_text(error)
                        .clicked();
                    ui.label(
                        egui::RichText::new(
                            text.auxiliary.managed_tools.tool_status_install_failed,
                        )
                        .color(theme.danger_text()),
                    )
                    .on_hover_text(error);
                }
            },
        );
    });

    ui.label(description);
    ui.label(
        egui::RichText::new(localized_version_label(tool, text))
            .monospace()
            .small()
            .color(egui::Color32::GRAY),
    );

    if install_clicked {
        start_install(download_manager, tool);
    }
    if remove_clicked {
        request_removal(download_manager, tool);
    }
    render_recoveries(ui, download_manager, text, tool, &theme);
}

fn localized_version_label(tool: ExternalTool, text: &LocaleText) -> String {
    version_label(tool).unwrap_or_else(|| {
        format!(
            "{} (x64)",
            text.auxiliary.managed_tools.tool_status_unavailable
        )
    })
}

fn render_recoveries(
    ui: &mut egui::Ui,
    download_manager: &DownloadManager,
    text: &LocaleText,
    tool: ExternalTool,
    theme: &AppTheme,
) {
    let recoveries = match external_tools::recoveries(tool) {
        Ok(recoveries) => recoveries,
        Err(error) => {
            ui.colored_label(
                theme.danger_text(),
                text.auxiliary
                    .managed_tools
                    .tool_recovery_inventory_error_fmt
                    .replace("{error}", &error.to_string()),
            );
            return;
        }
    };
    for recovery in recoveries {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.label(
                egui::RichText::new(text.auxiliary.managed_tools.tool_recovery_preserved)
                    .strong()
                    .color(theme.warning()),
            );
            ui.label(&recovery.reason);
            ui.label(
                egui::RichText::new(recovery.path.display().to_string())
                    .monospace()
                    .small(),
            );
            ui.horizontal(|ui| {
                if ui
                    .button(text.auxiliary.download.download_open_folder_btn)
                    .clicked()
                {
                    let path = if recovery.path.exists() {
                        recovery.path.as_path()
                    } else {
                        recovery.path.parent().unwrap_or(recovery.path.as_path())
                    };
                    let _ = open::that(path);
                }
                if recovery.can_clean
                    && ui
                        .button(text.auxiliary.managed_tools.tool_recovery_clean_verified)
                        .clicked()
                {
                    let feedback = match external_tools::clean_recovery(tool, &recovery) {
                        Ok(outcome) if outcome.preserved_paths.is_empty() => text
                            .auxiliary
                            .managed_tools
                            .tool_recovery_removed_fmt
                            .replace("{count}", &outcome.removed_files.to_string())
                            .replace("{path}", &outcome.path.display().to_string()),
                        Ok(outcome) => text
                            .auxiliary
                            .managed_tools
                            .tool_recovery_preserved_paths_fmt
                            .replace(
                                "{paths}",
                                &outcome
                                    .preserved_paths
                                    .iter()
                                    .map(|path| path.display().to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ),
                        Err(error) => text
                            .auxiliary
                            .managed_tools
                            .tool_recovery_cleanup_failed_fmt
                            .replace("{error}", &error.to_string()),
                    };
                    set_recovery_feedback(tool, feedback.clone());
                    if let Ok(mut logs) = download_manager.install_logs.lock() {
                        logs.push(feedback);
                    }
                }
            });
        });
    }
    if let Some(feedback) = recovery_feedback(tool) {
        ui.label(egui::RichText::new(feedback).small().color(theme.warning()));
    }
}

fn set_recovery_feedback(tool: ExternalTool, message: String) {
    let feedback = RECOVERY_FEEDBACK.get_or_init(|| Mutex::new(std::array::from_fn(|_| None)));
    if let Ok(mut feedback) = feedback.lock() {
        feedback[tool_index(tool)] = Some(message);
    }
}

fn recovery_feedback(tool: ExternalTool) -> Option<String> {
    RECOVERY_FEEDBACK
        .get()
        .and_then(|feedback| feedback.lock().ok())
        .and_then(|feedback| feedback[tool_index(tool)].clone())
}

const fn tool_index(tool: ExternalTool) -> usize {
    match tool {
        ExternalTool::YtDlp => 0,
        ExternalTool::Ffmpeg => 1,
        ExternalTool::Deno => 2,
    }
}

fn tool_status(download_manager: &DownloadManager, tool: ExternalTool) -> InstallStatus {
    match tool {
        ExternalTool::YtDlp => download_manager.ytdlp_status.lock().unwrap().clone(),
        ExternalTool::Ffmpeg => download_manager.ffmpeg_status.lock().unwrap().clone(),
        ExternalTool::Deno => download_manager.deno_status.lock().unwrap().clone(),
    }
}

fn set_tool_status(download_manager: &DownloadManager, tool: ExternalTool, status: InstallStatus) {
    let slot = match tool {
        ExternalTool::YtDlp => &download_manager.ytdlp_status,
        ExternalTool::Ffmpeg => &download_manager.ffmpeg_status,
        ExternalTool::Deno => &download_manager.deno_status,
    };
    *slot.lock().unwrap() = status;
}

fn start_install(download_manager: &DownloadManager, tool: ExternalTool) {
    match tool {
        ExternalTool::YtDlp => download_manager.start_download_ytdlp(),
        ExternalTool::Ffmpeg => download_manager.start_download_ffmpeg(),
        ExternalTool::Deno => download_manager.start_download_deno(),
    }
}

fn request_removal(download_manager: &DownloadManager, tool: ExternalTool) {
    match download_manager.remove_tool(tool) {
        Ok(()) => set_tool_status(download_manager, tool, InstallStatus::Missing),
        Err(error) => {
            let message = error.to_string();
            set_tool_status(
                download_manager,
                tool,
                InstallStatus::Error(message.clone()),
            );
            if let Ok(mut logs) = download_manager.install_logs.lock() {
                logs.push(format!("Could not remove {}: {message}", tool.id()));
            }
        }
    }
}
