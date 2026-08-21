use crate::component_registry::RemovalOutcome;
use crate::component_registry::external_tools::{self, ExternalTool, version_label};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus};
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::utils::{removal_in_progress, start_removal, tool_card};

static RECOVERY_FEEDBACK: OnceLock<Mutex<[Option<String>; 3]>> = OnceLock::new();
static RECOVERY_CACHE: OnceLock<Mutex<[RecoveryCacheEntry; 3]>> = OnceLock::new();
const RECOVERY_CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
struct RecoveryCacheEntry {
    value: Option<Result<Vec<external_tools::ExternalToolRecovery>, String>>,
    updated_at: Option<Instant>,
    loading: bool,
}

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
                            text.auxiliary.managed_tools.tool_status_installed.replace(
                                "{}",
                                &version_label(tool).unwrap_or_else(|| "x64".to_string()),
                            ),
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
                InstallStatus::Finalizing => {
                    ui.spinner();
                    ui.label(text.auxiliary.download.download_status_finalizing);
                }
                InstallStatus::Checking => {
                    ui.spinner();
                    ui.label(text.auxiliary.download.download_status_checking);
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
                InstallStatus::Unavailable => {
                    ui.label(
                        egui::RichText::new(text.auxiliary.managed_tools.tool_status_unavailable)
                            .color(theme.danger_text()),
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
    if let Some(version) = version_label(tool) {
        ui.label(
            egui::RichText::new(version)
                .monospace()
                .small()
                .color(egui::Color32::GRAY),
        );
    }

    if install_clicked {
        start_install(download_manager, tool);
    }
    if remove_clicked {
        request_removal(download_manager, tool);
    }
    render_recoveries(ui, download_manager, text, tool, &theme);
}

fn render_recoveries(
    ui: &mut egui::Ui,
    download_manager: &DownloadManager,
    text: &LocaleText,
    tool: ExternalTool,
    theme: &AppTheme,
) {
    let Some(cached) = cached_recoveries(tool) else {
        return;
    };
    let recoveries = match cached {
        Ok(recoveries) => recoveries,
        Err(error) => {
            ui.colored_label(
                theme.danger_text(),
                text.auxiliary
                    .managed_tools
                    .tool_recovery_inventory_error_fmt
                    .replace("{error}", &error),
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
                if removal_in_progress(recovery_removal_key(tool)) {
                    ui.spinner();
                    ui.label(text.auxiliary.managed_tools.tool_status_removing);
                } else if recovery.can_clean
                    && ui
                        .button(text.auxiliary.managed_tools.tool_recovery_clean_verified)
                        .clicked()
                {
                    start_recovery_cleanup(download_manager, text, tool, recovery.clone());
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

fn cached_recoveries(
    tool: ExternalTool,
) -> Option<Result<Vec<external_tools::ExternalToolRecovery>, String>> {
    let now = Instant::now();
    let cache = RECOVERY_CACHE
        .get_or_init(|| Mutex::new(std::array::from_fn(|_| RecoveryCacheEntry::default())));
    let mut cache = cache.lock().ok()?;
    let entry = &mut cache[tool_index(tool)];
    let fresh = entry
        .updated_at
        .is_some_and(|updated| now.duration_since(updated) < RECOVERY_CACHE_TTL);
    if !fresh && !entry.loading {
        entry.loading = true;
        std::thread::spawn(move || {
            let value = external_tools::recoveries(tool).map_err(|error| format!("{error:#}"));
            if let Ok(mut cache) = RECOVERY_CACHE
                .get_or_init(|| Mutex::new(std::array::from_fn(|_| RecoveryCacheEntry::default())))
                .lock()
            {
                cache[tool_index(tool)] = RecoveryCacheEntry {
                    value: Some(value),
                    updated_at: Some(Instant::now()),
                    loading: false,
                };
            }
        });
    }
    entry.value.clone()
}

fn invalidate_recoveries(tool: ExternalTool) {
    if let Some(cache) = RECOVERY_CACHE.get()
        && let Ok(mut cache) = cache.lock()
    {
        cache[tool_index(tool)].updated_at = None;
    }
}

fn tool_status(download_manager: &DownloadManager, tool: ExternalTool) -> InstallStatus {
    match tool {
        ExternalTool::YtDlp => download_manager.ytdlp_status.lock().unwrap().clone(),
        ExternalTool::Ffmpeg => download_manager.ffmpeg_status.lock().unwrap().clone(),
        ExternalTool::Deno => download_manager.deno_status.lock().unwrap().clone(),
    }
}

fn tool_status_slot(
    download_manager: &DownloadManager,
    tool: ExternalTool,
) -> Arc<Mutex<InstallStatus>> {
    match tool {
        ExternalTool::YtDlp => download_manager.ytdlp_status.clone(),
        ExternalTool::Ffmpeg => download_manager.ffmpeg_status.clone(),
        ExternalTool::Deno => download_manager.deno_status.clone(),
    }
}

fn start_install(download_manager: &DownloadManager, tool: ExternalTool) {
    match tool {
        ExternalTool::YtDlp => download_manager.start_download_ytdlp(),
        ExternalTool::Ffmpeg => download_manager.start_download_ffmpeg(),
        ExternalTool::Deno => download_manager.start_download_deno(),
    }
}

fn request_removal(download_manager: &DownloadManager, tool: ExternalTool) {
    download_manager.cancel_download();
    let status = tool_status_slot(download_manager, tool);
    let logs = download_manager.install_logs.clone();
    if let Ok(mut current) = status.lock() {
        *current = InstallStatus::Checking;
    }
    start_removal(
        tool_removal_key(tool),
        tool.id().to_string(),
        move || {
            let _recorder = crate::overlay::screen_record::stop_for_component_removal()?;
            let result = remove_external_tool(tool);
            let next = match &result {
                Ok(()) => InstallStatus::Missing,
                Err(error) => InstallStatus::Error(error.to_string()),
            };
            if let Ok(mut current) = status.lock() {
                *current = next;
            }
            if let Err(error) = &result
                && let Ok(mut logs) = logs.lock()
            {
                logs.push(format!("Could not remove {}: {error:#}", tool.id()));
            }
            result
        },
        move || invalidate_recoveries(tool),
    );
}

fn remove_external_tool(tool: ExternalTool) -> anyhow::Result<()> {
    match external_tools::remove(tool)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::RequiredBy(dependents) => anyhow::bail!(
            "{} is required by installed components: {}",
            tool.id(),
            dependents.join(", ")
        ),
        RemovalOutcome::PreservedModified(paths) => anyhow::bail!(
            "{} contains {} unrecorded or unsafe path(s); they were preserved",
            tool.id(),
            paths.len()
        ),
    }
}

fn start_recovery_cleanup(
    download_manager: &DownloadManager,
    text: &LocaleText,
    tool: ExternalTool,
    recovery: external_tools::ExternalToolRecovery,
) {
    let logs = download_manager.install_logs.clone();
    let removed_template = text
        .auxiliary
        .managed_tools
        .tool_recovery_removed_fmt
        .to_string();
    let preserved_template = text
        .auxiliary
        .managed_tools
        .tool_recovery_preserved_paths_fmt
        .to_string();
    start_removal(
        recovery_removal_key(tool),
        tool.id().to_string(),
        move || {
            let outcome = external_tools::clean_recovery(tool, &recovery)?;
            let feedback = if outcome.preserved_paths.is_empty() {
                removed_template
                    .replace("{count}", &outcome.removed_files.to_string())
                    .replace("{path}", &outcome.path.display().to_string())
            } else {
                preserved_template.replace(
                    "{paths}",
                    &outcome
                        .preserved_paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            };
            set_recovery_feedback(tool, feedback.clone());
            if let Ok(mut logs) = logs.lock() {
                logs.push(feedback);
            }
            Ok(())
        },
        move || invalidate_recoveries(tool),
    );
}

const fn tool_removal_key(tool: ExternalTool) -> &'static str {
    match tool {
        ExternalTool::YtDlp => "downloaded-tools:remove-yt-dlp",
        ExternalTool::Ffmpeg => "downloaded-tools:remove-ffmpeg",
        ExternalTool::Deno => "downloaded-tools:remove-deno",
    }
}

const fn recovery_removal_key(tool: ExternalTool) -> &'static str {
    match tool {
        ExternalTool::YtDlp => "downloaded-tools:clean-yt-dlp-recovery",
        ExternalTool::Ffmpeg => "downloaded-tools:clean-ffmpeg-recovery",
        ExternalTool::Deno => "downloaded-tools:clean-deno-recovery",
    }
}
