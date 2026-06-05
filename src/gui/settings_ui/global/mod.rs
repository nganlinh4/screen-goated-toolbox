use super::node_graph::request_node_graph_view_reset;
use crate::config::Config;
use crate::gui::icons::{Icon, draw_icon_static};
use crate::gui::locale::LocaleText;
use crate::updater::{UpdateStatus, Updater};
use auto_launch::AutoLaunch;
use eframe::egui;
use std::collections::HashMap;

mod api_keys;
mod downloaded_tools;
mod model_priority;
mod tts_settings;
mod update_section;
mod usage_stats;

use crate::gui::settings_ui::download_manager::DownloadManager;
use api_keys::{ApiKeyCardStyle, ApiKeyVisibility, render_api_keys_card};
use downloaded_tools::render_downloaded_tools_modal;
use model_priority::render_model_priority_modal;
use tts_settings::render_tts_settings_modal;
use update_section::render_update_section_content;
use usage_stats::render_usage_modal;

#[expect(
    clippy::too_many_arguments,
    reason = "settings renderer receives independent feature toggles and shared state from the parent UI"
)]
pub fn render_global_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    show_api_key: &mut bool,
    show_gemini_api_key: &mut bool,
    show_openrouter_api_key: &mut bool,
    show_cerebras_api_key: &mut bool,
    usage_stats: &HashMap<String, String>,
    updater: &Option<Updater>,
    update_status: &UpdateStatus,
    run_at_startup: &mut bool,
    auto_launcher: &Option<AutoLaunch>,
    current_admin_state: bool,
    text: &LocaleText,
    show_usage_modal: &mut bool,

    show_tts_modal: &mut bool,
    show_tools_modal: &mut bool,
    show_model_priority_modal: &mut bool,
    download_manager: &mut DownloadManager,
    _cached_audio_devices: &std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    _recording_sr_hotkey: &mut bool,
) -> bool {
    let mut changed = false;

    let is_dark = ui.visuals().dark_mode;
    let theme = crate::gui::theme::AppTheme::from_dark(is_dark);
    let card_bg = theme.card_bg();
    let card_stroke = theme.card_stroke();

    ui.add_space(5.0);

    if render_api_keys_card(
        ui,
        config,
        ApiKeyVisibility {
            groq: show_api_key,
            gemini: show_gemini_api_key,
            openrouter: show_openrouter_api_key,
            cerebras: show_cerebras_api_key,
        },
        text,
        ApiKeyCardStyle {
            background: card_bg,
            stroke: card_stroke,
        },
    ) {
        changed = true;
    }

    ui.add_space(10.0);

    // === USAGE STATISTICS & TTS SETTINGS BUTTONS ===
    let on_btn = theme.on_accent();

    ui.horizontal(|ui| {
        if crate::gui::widgets::filled_icon_button(
            ui,
            Icon::BarChart,
            text.usage_statistics_title,
            theme.btn_stats(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(text.usage_statistics_tooltip)
        .clicked()
        {
            *show_usage_modal = true;
        }

        ui.add_space(10.0);

        if crate::gui::widgets::filled_icon_button(
            ui,
            Icon::SettingsVoice,
            text.tts_settings_button,
            theme.btn_tts_settings(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        {
            *show_tts_modal = true;
        }

        ui.add_space(10.0);

        if crate::gui::widgets::filled_icon_button(
            ui,
            Icon::Download,
            text.downloaded_tools_button,
            theme.btn_tools(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        {
            *show_tools_modal = true;
        }
    });

    ui.add_space(10.0);

    ui.horizontal(|ui| {
        if crate::gui::widgets::filled_icon_button(
            ui,
            Icon::Priority,
            text.model_priority_button,
            theme.btn_priority(),
            on_btn,
            10,
        )
        .clicked()
        {
            *show_model_priority_modal = true;
        }

        ui.add_space(10.0);

        // Help assistant — shares the Model Priority row (its teal accent kept).
        if crate::gui::widgets::filled_icon_button(
            ui,
            Icon::AutoStories,
            text.help_assistant_btn,
            theme.accent_help(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(text.help_assistant_title)
        .clicked()
        {
            std::thread::spawn(|| {
                crate::gui::settings_ui::help_assistant::show_help_input();
            });
        }
    });

    // === USAGE STATISTICS MODAL ===
    render_usage_modal(
        ui,
        usage_stats,
        text,
        show_usage_modal,
        config.use_groq,
        config.use_gemini,
        config.use_openrouter,
        config.use_ollama,
        config.use_cerebras,
    );

    // === TOOLS MODAL ===
    let ctx = ui.ctx().clone();
    render_downloaded_tools_modal(&ctx, ui, show_tools_modal, download_manager, text);

    // === TTS SETTINGS MODAL ===
    if render_tts_settings_modal(ui, config, text, show_tts_modal) {
        changed = true;
    }

    if render_model_priority_modal(ui, config, text, show_model_priority_modal) {
        changed = true;
    }

    ui.add_space(10.0);

    // === SOFTWARE UPDATE CARD ===
    egui::Frame::new()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Upgrade, Some(crate::gui::icons::ICON_MD));
                ui.label(
                    egui::RichText::new(text.software_update_header)
                        .strong()
                        .size(14.0),
                );
            });
            ui.add_space(6.0);
            render_update_section_content(ui, updater, update_status, text);
        });

    ui.add_space(10.0);

    // === STARTUP OPTIONS CARD ===
    egui::Frame::new()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Settings, Some(crate::gui::icons::ICON_MD));
                ui.label(
                    egui::RichText::new(text.startup_display_header)
                        .strong()
                        .size(14.0),
                );
            });
            ui.add_space(6.0);

            // Main startup toggle
            ui.horizontal(|ui| {
                if let Some(launcher) = auto_launcher {
                    let mut startup_toggle = *run_at_startup;
                    if ui
                        .checkbox(&mut startup_toggle, text.startup_label)
                        .clicked()
                    {
                        if startup_toggle && !(*run_at_startup) {
                            // User is turning it ON - authorize THIS exe as the one allowed to start
                            if let Ok(exe_path) = std::env::current_exe()
                                && let Some(exe_str) = exe_path.to_str()
                            {
                                config.authorized_startup_path = exe_str.to_string();
                            }

                            if config.run_as_admin_on_startup && current_admin_state {
                                if crate::gui::utils::set_admin_startup(true) {
                                    let _ = launcher.disable();
                                    config.run_at_startup = false;
                                    *run_at_startup = true;
                                    changed = true;
                                }
                            } else {
                                std::thread::spawn(|| {
                                    crate::gui::utils::set_admin_startup(false);
                                });
                                let _ = launcher.enable();
                                config.run_at_startup = true;
                                *run_at_startup = true;
                                changed = true;
                            }
                        } else if !startup_toggle && *run_at_startup {
                            // User is turning it OFF
                            std::thread::spawn(|| {
                                crate::gui::utils::set_admin_startup(false);
                            });
                            let _ = launcher.disable();
                            config.run_as_admin_on_startup = false;
                            config.run_at_startup = false;
                            config.start_in_tray = false;
                            *run_at_startup = false;
                            changed = true;
                        }
                    }
                }
            });

            // Admin Mode Sub-option
            if *run_at_startup {
                ui.indent("admin_indent", |ui| {
                    let mut is_admin_mode = config.run_as_admin_on_startup;
                    let checkbox_label = text.admin_startup_on;

                    if current_admin_state {
                        if ui.checkbox(&mut is_admin_mode, checkbox_label).clicked() {
                            if is_admin_mode && !config.run_as_admin_on_startup {
                                // Transitioning to admin mode requires updated authorization
                                if let Ok(exe_path) = std::env::current_exe()
                                    && let Some(exe_str) = exe_path.to_str()
                                {
                                    config.authorized_startup_path = exe_str.to_string();
                                }

                                if crate::gui::utils::set_admin_startup(true) {
                                    config.run_as_admin_on_startup = true;
                                    config.run_at_startup = false;
                                    if let Some(launcher) = auto_launcher {
                                        let _ = launcher.disable();
                                    }
                                    changed = true;
                                }
                            } else if !is_admin_mode && config.run_as_admin_on_startup {
                                // Reverting to standard mode
                                if let Ok(exe_path) = std::env::current_exe()
                                    && let Some(exe_str) = exe_path.to_str()
                                {
                                    config.authorized_startup_path = exe_str.to_string();
                                }

                                std::thread::spawn(|| {
                                    crate::gui::utils::set_admin_startup(false);
                                });
                                config.run_as_admin_on_startup = false;
                                config.run_at_startup = true;
                                if let Some(launcher) = auto_launcher {
                                    let _ = launcher.enable();
                                }
                                changed = true;
                            }
                        }
                    } else {
                        let mut _is_admin_mode_disabled = config.run_as_admin_on_startup;
                        ui.add_enabled_ui(false, |ui| {
                            ui.checkbox(&mut _is_admin_mode_disabled, checkbox_label);
                        });
                        ui.label(
                            egui::RichText::new(text.admin_startup_fail)
                                .size(11.0)
                                .color(theme.warning()),
                        );
                    }

                    if config.run_as_admin_on_startup && current_admin_state {
                        ui.label(
                            egui::RichText::new(text.admin_startup_success)
                                .size(11.0)
                                .color(theme.success()),
                        );
                    }
                });

                if ui
                    .checkbox(&mut config.start_in_tray, text.start_in_tray_label)
                    .clicked()
                {
                    changed = true;
                }
            }

            ui.add_space(8.0);

            config.favorite_overlay_opacity = config.favorite_overlay_opacity.clamp(10, 100);
            ui.horizontal(|ui| {
                ui.label(text.favorite_overlay_opacity_label);
                if ui
                    .add(
                        egui::Slider::new(&mut config.favorite_overlay_opacity, 10..=100)
                            .suffix("%"),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            ui.add_space(8.0);

            // Graphics Mode + Reset button on same row
            ui.horizontal(|ui| {
                let current_label = match config.ui_language.as_str() {
                    "vi" => {
                        if config.graphics_mode == "minimal" {
                            "Tối giản"
                        } else {
                            "Tiêu chuẩn"
                        }
                    }
                    "ko" => {
                        if config.graphics_mode == "minimal" {
                            "최소"
                        } else {
                            "표준"
                        }
                    }
                    _ => {
                        if config.graphics_mode == "minimal" {
                            "Minimal"
                        } else {
                            "Standard"
                        }
                    }
                };

                crate::gui::widgets::combo("graphics_mode_combo")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                config.graphics_mode == "standard",
                                text.graphics_mode_standard,
                            )
                            .clicked()
                        {
                            config.graphics_mode = "standard".to_string();
                            changed = true;
                        }
                        if ui
                            .selectable_label(
                                config.graphics_mode == "minimal",
                                text.graphics_mode_minimal,
                            )
                            .clicked()
                        {
                            config.graphics_mode = "minimal".to_string();
                            changed = true;
                        }
                    });

                // Big gap to simulate right alignment
                ui.add_space(40.0);

                // Force Quit button — amber (less drastic than the red factory reset).
                if crate::gui::widgets::filled_button(
                    ui,
                    text.force_quit,
                    theme.warning_fill(),
                    theme.on_accent(),
                    8,
                )
                .clicked()
                {
                    crate::gui::app::exit_app();
                }

                ui.add_space(10.0);

                // Reset Defaults button — red: a factory reset wipes everything, so
                // it's the most alarming action (distinct from the amber Force Quit).
                if crate::gui::widgets::filled_button(
                    ui,
                    text.reset_defaults_btn,
                    theme.danger_fill(),
                    theme.on_accent(),
                    8,
                )
                .clicked()
                {
                    let saved_groq_key = config.api_key.clone();
                    let saved_gemini_key = config.gemini_api_key.clone();
                    let saved_openrouter_key = config.openrouter_api_key.clone();
                    let saved_cerebras_key = config.cerebras_api_key.clone();
                    let saved_language = config.ui_language.clone();
                    let saved_use_groq = config.use_groq;
                    let saved_use_gemini = config.use_gemini;
                    let saved_use_openrouter = config.use_openrouter;
                    let saved_use_ollama = config.use_ollama;
                    let saved_use_cerebras = config.use_cerebras;
                    let saved_ollama_base_url = config.ollama_base_url.clone();

                    *config = Config::default();

                    config.api_key = saved_groq_key;
                    config.gemini_api_key = saved_gemini_key;
                    config.openrouter_api_key = saved_openrouter_key;
                    config.cerebras_api_key = saved_cerebras_key;
                    config.ui_language = saved_language;
                    config.use_groq = saved_use_groq;
                    config.use_gemini = saved_use_gemini;
                    config.use_openrouter = saved_use_openrouter;
                    config.use_ollama = saved_use_ollama;
                    config.use_cerebras = saved_use_cerebras;
                    config.ollama_base_url = saved_ollama_base_url;
                    // config.realtime_translation_model = saved_realtime_model;
                    request_node_graph_view_reset(ui.ctx());

                    // Full factory reset: wipe every app-managed directory
                    // (recordings, downloaded runtime DLLs, models, caches,
                    // pointer packs, backgrounds, webview-selector, legacy
                    // orphans). SGT/webview_data is still locked by the
                    // running process, so that one is scheduled for startup.
                    crate::overlay::clear_all_app_data();
                    config.clear_webview_on_startup = true;

                    // Save immediately and restart
                    crate::config::save_config(config);
                    crate::gui::app::restart_app();

                    changed = true;
                }
            });
        });

    ui.add_space(10.0);

    ui.add_space(10.0);

    changed
}
