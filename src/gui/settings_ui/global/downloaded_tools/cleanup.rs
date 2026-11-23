use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;
use eframe::egui;

use crate::component_registry::RemovalOutcome;
use crate::component_registry::external_tools::{ExternalTool, ExternalToolStatus};
use crate::gui::locale::{LocaleText, ManagedToolsLocaleText};
use crate::gui::settings_ui::download_manager::{DownloadManager, InstallStatus};
use crate::gui::settings_ui::{ConfirmModal, ConfirmResult};
use crate::gui::theme::AppTheme;

mod completion;

const CLEANUP_STEP_COUNT: usize = 6;

#[derive(Clone, Copy)]
enum CleanupStep {
    Interfaces,
    Components,
    Recoveries,
    Models,
    Runtimes,
    Media,
}

impl CleanupStep {
    fn label(self, text: &ManagedToolsLocaleText) -> &'static str {
        match self {
            Self::Interfaces => text.downloaded_tools_clean_step_interfaces,
            Self::Components => text.downloaded_tools_clean_step_components,
            Self::Recoveries => text.downloaded_tools_clean_step_recoveries,
            Self::Models => text.downloaded_tools_clean_step_models,
            Self::Runtimes => text.downloaded_tools_clean_step_runtimes,
            Self::Media => text.downloaded_tools_clean_step_media,
        }
    }
}

#[derive(Clone)]
enum CleanupDialogState {
    Idle,
    Confirming,
    Running { completed: usize, step: CleanupStep },
    Complete { attention_count: usize },
}

static CLEANUP_STATE: LazyLock<Mutex<CleanupDialogState>> =
    LazyLock::new(|| Mutex::new(CleanupDialogState::Idle));

#[derive(Clone)]
struct StatusTargets {
    ytdlp: Arc<Mutex<InstallStatus>>,
    ffmpeg: Arc<Mutex<InstallStatus>>,
    deno: Arc<Mutex<InstallStatus>>,
}

impl StatusTargets {
    fn capture(manager: &DownloadManager) -> Self {
        Self {
            ytdlp: manager.ytdlp_status.clone(),
            ffmpeg: manager.ffmpeg_status.clone(),
            deno: manager.deno_status.clone(),
        }
    }

    fn refresh(&self) {
        for (tool, target) in [
            (ExternalTool::YtDlp, &self.ytdlp),
            (ExternalTool::Ffmpeg, &self.ffmpeg),
            (ExternalTool::Deno, &self.deno),
        ] {
            set_status(target, external_tool_status(tool));
        }
    }
}

pub(super) fn request_confirmation() {
    let mut state = CLEANUP_STATE
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if matches!(*state, CleanupDialogState::Idle) {
        *state = CleanupDialogState::Confirming;
    }
}

pub(super) fn is_active() -> bool {
    !matches!(
        *CLEANUP_STATE
            .lock()
            .unwrap_or_else(|value| value.into_inner()),
        CleanupDialogState::Idle
    )
}

pub(super) fn render(ctx: &egui::Context, text: &LocaleText, download_manager: &DownloadManager) {
    let state = CLEANUP_STATE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clone();
    match state {
        CleanupDialogState::Idle => {}
        CleanupDialogState::Confirming => render_confirmation(ctx, text, download_manager),
        CleanupDialogState::Running { completed, step } => {
            render_progress(ctx, text, completed, step)
        }
        CleanupDialogState::Complete { attention_count } => {
            render_complete(ctx, text, attention_count)
        }
    }
}

fn render_confirmation(ctx: &egui::Context, text: &LocaleText, download_manager: &DownloadManager) {
    let managed = &text.auxiliary.managed_tools;
    let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
    match ConfirmModal::new(
        egui::Id::new("downloaded_tools_clean_confirm"),
        managed.downloaded_tools_clean_confirm_title,
        managed.downloaded_tools_clean_confirm_body,
    )
    .labels(
        managed.downloaded_tools_clean_confirm,
        managed.downloaded_tools_clean_cancel,
    )
    .destructive(true)
    .show_ctx(ctx, &theme)
    {
        ConfirmResult::Confirmed => {
            download_manager.cancel_all_activity();
            start_cleanup(ctx.clone(), StatusTargets::capture(download_manager));
        }
        ConfirmResult::Cancelled => set_state(CleanupDialogState::Idle),
        ConfirmResult::Pending => {}
    }
}

fn render_progress(ctx: &egui::Context, text: &LocaleText, completed: usize, step: CleanupStep) {
    let managed = &text.auxiliary.managed_tools;
    let visible_step = (completed + 1).min(CLEANUP_STEP_COUNT);
    let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
    crate::gui::widgets::material_modal(
        ctx,
        &theme,
        egui::Id::new("downloaded_tools_clean_progress"),
        |ui| {
            ui.set_min_width(420.0);
            crate::gui::widgets::dialog_title(
                ui,
                &theme,
                managed.downloaded_tools_clean_progress_title,
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                crate::gui::widgets::dialog_body(
                    ui,
                    &theme,
                    &managed
                        .downloaded_tools_clean_progress_fmt
                        .replace("{done}", &visible_step.to_string())
                        .replace("{total}", &CLEANUP_STEP_COUNT.to_string())
                        .replace("{item}", step.label(managed)),
                );
            });
            ui.add(
                egui::ProgressBar::new(cleanup_fraction(completed))
                    .animate(true)
                    .desired_width(390.0),
            );
        },
    );
}

fn render_complete(ctx: &egui::Context, text: &LocaleText, attention_count: usize) {
    let managed = &text.auxiliary.managed_tools;
    let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
    let modal = crate::gui::widgets::material_modal(
        ctx,
        &theme,
        egui::Id::new("downloaded_tools_clean_complete"),
        |ui| {
            ui.set_min_width(420.0);
            crate::gui::widgets::dialog_title(
                ui,
                &theme,
                managed.downloaded_tools_clean_progress_title,
            );
            ui.add_space(8.0);
            let message = if attention_count == 0 {
                managed.downloaded_tools_clean_complete.to_string()
            } else {
                managed
                    .downloaded_tools_clean_complete_with_errors_fmt
                    .replace("{count}", &attention_count.to_string())
            };
            crate::gui::widgets::dialog_body(ui, &theme, &message);
            ui.add_space(12.0);
            if crate::gui::widgets::filled_button(
                ui,
                managed.downloaded_tools_clean_close,
                theme.neutral_fill(),
                theme.on_surface(),
                16,
            )
            .clicked()
            {
                set_state(CleanupDialogState::Idle);
            }
        },
    );
    if modal.should_close() {
        set_state(CleanupDialogState::Idle);
    }
}

fn start_cleanup(ctx: egui::Context, targets: StatusTargets) {
    set_state(CleanupDialogState::Running {
        completed: 0,
        step: CleanupStep::Interfaces,
    });
    let completion = completion::CleanupCompletion::new(ctx.clone(), move || targets.refresh());
    if let Err(error) = std::thread::Builder::new()
        .name("sgt-cleanup".into())
        .spawn(move || {
            let mut completion = completion;
            let attention_count =
                match crate::install_activity::begin_quiescence(std::time::Duration::from_secs(30))
                {
                    Ok(quiescence) => {
                        completion.hold_quiescence(quiescence);
                        completion.mark_started();
                        run_cleanup(&ctx)
                    }
                    Err(error) => {
                        crate::log_info!("[Downloaded Tools] could not stop installers: {error:#}");
                        1
                    }
                };
            completion.finish(attention_count);
        })
    {
        crate::log_info!("[Downloaded Tools] could not start cleanup: {error}");
    }
}

fn run_cleanup(ctx: &egui::Context) -> usize {
    let mut attention_count = 0;
    run_step(ctx, 0, CleanupStep::Interfaces, || {
        record_result(
            "PromptDJ interface",
            crate::overlay::prompt_dj::remove_web_assets(),
            &mut attention_count,
        );
        record_result(
            "TTS Playground interface",
            crate::overlay::tts_playground::remove_web_assets(),
            &mut attention_count,
        );
        record_result(
            "Creation tools",
            crate::overlay::three_d_generator::remove_product(),
            &mut attention_count,
        );
        record_result(
            "recorder",
            crate::component_registry::recorder::remove_all(),
            &mut attention_count,
        );
        record_result(
            "Computer Control integrations",
            crate::overlay::computer_control::ui_remove_all(),
            &mut attention_count,
        );
        record_result(
            "Computer Control",
            crate::overlay::computer_control::remove_downloaded_engine(),
            &mut attention_count,
        );
        record_result(
            "Screen Translate detector",
            crate::component_registry::screen_text_detector::remove(),
            &mut attention_count,
        );
        crate::overlay::stop_realtime_overlay();
        crate::overlay::recording::compositor_cancel();
        crate::api::tts::TTS_MANAGER.stop();
    });
    run_step(
        ctx,
        1,
        CleanupStep::Components,
        || match crate::component_registry::clean_all(&[]) {
            Ok(outcomes) => {
                for (id, outcome) in outcomes {
                    if matches!(
                        outcome,
                        RemovalOutcome::RequiredBy(_) | RemovalOutcome::PreservedModified(_)
                    ) {
                        attention_count += 1;
                        crate::log_info!(
                            "[Downloaded Tools] component {id} needs attention: {outcome:?}"
                        );
                    }
                }
            }
            Err(error) => record_error("managed components", error, &mut attention_count),
        },
    );
    run_step(ctx, 2, CleanupStep::Recoveries, || {
        match crate::component_registry::recovery::purge_all_recorded() {
            Ok(outcomes) => {
                for outcome in outcomes {
                    if !outcome.preserved_paths.is_empty() {
                        attention_count += 1;
                        crate::log_info!(
                            "[Downloaded Tools] recovery at {} preserved {} path(s)",
                            outcome.path.display(),
                            outcome.preserved_paths.len()
                        );
                    }
                }
            }
            Err(error) => record_error("component recovery files", error, &mut attention_count),
        }
        match crate::component_registry::recorder::purge_all_recorded_recoveries() {
            Ok(outcomes) => {
                for outcome in outcomes {
                    if !outcome.preserved_paths.is_empty() {
                        attention_count += 1;
                        crate::log_info!(
                            "[Downloaded Tools] recorder recovery at {} preserved {} path(s)",
                            outcome.path.display(),
                            outcome.preserved_paths.len()
                        );
                    }
                }
            }
            Err(error) => record_error("recorder recovery files", error, &mut attention_count),
        }
        match crate::component_registry::external_tools::purge_all_recorded_recoveries() {
            Ok(outcomes) => {
                for outcome in outcomes {
                    if !outcome.preserved_paths.is_empty() {
                        attention_count += 1;
                    }
                }
            }
            Err(error) => record_error("recovery files", error, &mut attention_count),
        }
    });
    run_step(ctx, 3, CleanupStep::Models, || {
        record_result(
            "legacy offline models",
            crate::component_registry::legacy_offline::remove(),
            &mut attention_count,
        );
    });
    run_step(ctx, 4, CleanupStep::Runtimes, || {
        record_result(
            "legacy AI runtime",
            crate::unpack_dlls::remove_ai_runtime(),
            &mut attention_count,
        );
    });
    run_step(ctx, 5, CleanupStep::Media, || {
        record_result(
            "downloaded recorder backgrounds",
            crate::overlay::screen_record::bg_download::delete_all_downloaded()
                .map(drop)
                .map_err(anyhow::Error::msg),
            &mut attention_count,
        );
        record_result(
            "downloaded pointer collections",
            crate::gui::settings_ui::pointer_gallery::delete_downloaded_collections()
                .map(drop)
                .map_err(anyhow::Error::msg),
            &mut attention_count,
        );
    });
    attention_count
}

fn run_step(ctx: &egui::Context, completed: usize, step: CleanupStep, action: impl FnOnce()) {
    set_state(CleanupDialogState::Running { completed, step });
    ctx.request_repaint();
    action();
}

fn record_result(label: &str, result: Result<()>, attention_count: &mut usize) {
    if let Err(error) = result {
        record_error(label, error, attention_count);
    }
}

fn record_error(label: &str, error: anyhow::Error, attention_count: &mut usize) {
    *attention_count += 1;
    crate::log_info!("[Downloaded Tools] {label} cleanup failed: {error:#}");
}

fn external_tool_status(tool: ExternalTool) -> InstallStatus {
    match crate::component_registry::external_tools::current_status(tool) {
        ExternalToolStatus::Installed { .. } => InstallStatus::Installed,
        ExternalToolStatus::Missing => InstallStatus::Missing,
        ExternalToolStatus::Unavailable => InstallStatus::Unavailable,
        ExternalToolStatus::Error(error) => InstallStatus::Error(error),
    }
}

fn set_status(target: &Arc<Mutex<InstallStatus>>, status: InstallStatus) {
    if let Ok(mut current) = target.lock() {
        *current = status;
    }
}

fn cleanup_fraction(completed: usize) -> f32 {
    completed.min(CLEANUP_STEP_COUNT) as f32 / CLEANUP_STEP_COUNT as f32
}

fn set_state(next: CleanupDialogState) {
    *CLEANUP_STATE
        .lock()
        .unwrap_or_else(|value| value.into_inner()) = next;
}

#[cfg(test)]
mod tests;
