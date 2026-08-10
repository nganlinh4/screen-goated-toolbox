use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::DownloadManager;
use crate::gui::theme::AppTheme;
use eframe::egui;
use std::time::{Duration, Instant};

mod ai_runtime;
mod backgrounds;
mod computer_control;
mod mcp;
mod model_card;
mod model_sections;
mod pointer_packs;
mod recorder;
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
    three_d_generator::render_three_d_generator_card,
    tts_models::{render_magpie_card, render_step_audio_card, render_vieneu_card},
    utils::clear_downloaded_tools_caches,
    video_downloader::render_video_downloader_card,
    web_apps::render_web_apps_card,
    zipformer::render_zipformer_section,
};
use crate::component_registry::RemovalOutcome;
use crate::component_registry::external_tools::ExternalTool;
use crate::gui::settings_ui::download_manager::InstallStatus;

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
                            .button(
                                egui::RichText::new(
                                    text.auxiliary.managed_tools.downloaded_tools_clean_all,
                                )
                                .color(theme.danger_text()),
                            )
                            .clicked()
                        {
                            clean_all_downloaded_tools(download_manager);
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
}

fn clean_all_downloaded_tools(download_manager: &mut DownloadManager) {
    if let Err(error) = crate::component_registry::recorder::remove_all() {
        crate::log_info!("[Downloaded Tools] recorder cleanup failed: {error:#}");
    }
    let managed_outcomes = match crate::component_registry::clean_all() {
        Ok(outcomes) => Some(outcomes),
        Err(error) => {
            crate::log_info!("[Downloaded Tools] managed component cleanup failed: {error}");
            None
        }
    };
    match crate::component_registry::external_tools::clean_all_recoveries() {
        Ok(outcomes) => {
            for outcome in outcomes {
                if outcome.preserved_paths.is_empty() {
                    crate::log_info!(
                        "[Downloaded Tools] removed {} verified recovery file(s) from {}",
                        outcome.removed_files,
                        outcome.path.display()
                    );
                } else {
                    for path in outcome.preserved_paths {
                        crate::log_info!(
                            "[Downloaded Tools] preserved recovery path {}",
                            path.display()
                        );
                    }
                }
            }
        }
        Err(error) => {
            crate::log_info!("[Downloaded Tools] recovery cleanup failed: {error:#}");
        }
    }

    let _ = crate::unpack_dlls::remove_ai_runtime();
    let _ = crate::api::realtime_audio::model_loader::remove_parakeet_model();
    let _ = crate::api::realtime_audio::parakeet_tdt_assets::remove_parakeet_tdt_model();
    let _ = crate::api::realtime_audio::qwen3::assets::remove_qwen3_model();
    let _ = crate::api::realtime_audio::qwen3::assets::remove_qwen3_1_7b_model();
    remove_archived_qwen_server();
    let _ = crate::api::realtime_audio::qwen3::runtime::remove_qwen3_runtime();
    let _ = crate::api::realtime_audio::supertonic_assets::remove_supertonic_model();
    let _ = crate::api::realtime_audio::kokoro_assets::remove_kokoro_model();
    let _ = crate::api::realtime_audio::step_audio_assets::remove_step_audio_model();
    let _ = crate::api::realtime_audio::step_audio_runtime::remove_step_audio_runtime();
    let _ = crate::api::realtime_audio::magpie_assets::remove_magpie_model();
    let _ = crate::api::realtime_audio::magpie_runtime::remove_magpie_runtime();
    let _ = crate::api::realtime_audio::vieneu_assets::remove_vieneu_model();
    let _ = crate::api::realtime_audio::vieneu_runtime::remove_vieneu_runtime();
    let _ = crate::overlay::creation_runtime::remove_runtime();
    crate::overlay::computer_control::ui_remove_all(); // uninstall + forget MCP integrations
    let _ =
        std::fs::remove_dir_all(crate::api::realtime_audio::sherpa_onnx::dlls::sherpa_bin_dir());
    for lang in [
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::English,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::Korean,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::Chinese,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::French,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::German,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::Spanish,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::Russian,
        crate::api::realtime_audio::sherpa_onnx::ZipformerLanguage::All8Lang,
    ] {
        let model_dir = crate::paths::app_models_dir().join(lang.model_dir_name());
        let _ = std::fs::remove_dir_all(model_dir);
    }

    let _ = crate::overlay::screen_record::bg_download::delete_all_downloaded();
    let _ = crate::gui::settings_ui::pointer_gallery::delete_downloaded_collections();

    if let Some(outcomes) = managed_outcomes.as_deref() {
        apply_external_tool_cleanup_status(download_manager, outcomes);
    } else {
        download_manager.check_status();
    }
    set_install_status_missing(&download_manager.zipformer_dlls_status);
    for status in download_manager.zipformer_lang_statuses.values() {
        set_install_status_missing(status);
    }
    clear_downloaded_tools_caches();
}

fn apply_external_tool_cleanup_status(
    download_manager: &DownloadManager,
    outcomes: &[(String, RemovalOutcome)],
) {
    for tool in ExternalTool::ALL {
        let outcome = outcomes
            .iter()
            .find_map(|(id, outcome)| (id == tool.id()).then_some(outcome));
        let status = match outcome {
            None
            | Some(RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending) => {
                InstallStatus::Missing
            }
            Some(RemovalOutcome::RequiredBy(dependents)) => InstallStatus::Error(format!(
                "{} is still required by: {}",
                tool.id(),
                dependents.join(", ")
            )),
            Some(RemovalOutcome::PreservedModified(paths)) => InstallStatus::Error(format!(
                "{} preserved {} modified or unknown file(s)",
                tool.id(),
                paths.len()
            )),
        };
        let slot = match tool {
            ExternalTool::YtDlp => &download_manager.ytdlp_status,
            ExternalTool::Ffmpeg => &download_manager.ffmpeg_status,
            ExternalTool::Deno => &download_manager.deno_status,
        };
        if let Ok(mut current) = slot.lock() {
            *current = status;
        }
    }
}

fn remove_archived_qwen_server() {
    let directory = crate::paths::app_data_dir()
        .join("bin")
        .join("qwen3_asr_reference");
    let _ = std::fs::remove_file(directory.join("asr-server.exe"));
    let _ = std::fs::remove_dir(directory);
}

fn set_install_status_missing(status: &std::sync::Arc<std::sync::Mutex<InstallStatus>>) {
    if let Ok(mut status) = status.lock() {
        *status = InstallStatus::Missing;
    }
}
