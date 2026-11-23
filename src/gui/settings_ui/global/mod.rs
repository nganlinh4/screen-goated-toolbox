use crate::config::Config;
use crate::gui::icons::{Icon, draw_icon_static};
use crate::gui::locale::LocaleText;
use crate::gui::startup::Startup;
use crate::updater::{UpdateStatus, Updater};
use eframe::egui;

mod api_keys;
mod custom_models;
mod donate_section;
mod downloaded_tools;
mod model_priority;
mod models_modal;
pub use models_modal::ModelsTab;
mod restore_defaults;
mod startup;
mod tts_settings;
mod update_section;
mod usage_stats;

use crate::gui::settings_ui::download_manager::DownloadManager;
use api_keys::{ApiKeyCardStyle, ApiKeyVisibility, render_api_keys_card};
use donate_section::render_donate_section_content;
use downloaded_tools::render_downloaded_tools_modal;
use models_modal::{ModelsHubState, ProviderEnabled, render_models_modal};
pub(crate) use restore_defaults::complete_restore;
use restore_defaults::render_restore_defaults_modal;
use tts_settings::render_tts_settings_modal;
use update_section::render_update_section_content;

#[expect(
    clippy::too_many_arguments,
    reason = "settings renderer receives independent feature toggles and shared state from the parent UI"
)]
pub(crate) fn render_global_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    show_api_key: &mut bool,
    show_gemini_api_key: &mut bool,
    show_openrouter_api_key: &mut bool,
    show_nvidia_api_key: &mut bool,
    usage_stats: &crate::usage_stats::UsageStore,
    updater: &Option<Updater>,
    update_status: &UpdateStatus,
    run_at_startup: &mut bool,
    startup: &mut Startup,
    current_admin_state: bool,
    text: &LocaleText,
    show_models_modal: &mut bool,
    models_tab: &mut ModelsTab,
    show_tts_modal: &mut bool,
    show_tools_modal: &mut bool,
    show_restore_defaults_modal: &mut bool,
    download_manager: &mut DownloadManager,
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
            nvidia: show_nvidia_api_key,
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

    // === HUB BUTTONS: Models / Tools / Voice / Help (single row) ===
    let on_btn = theme.on_accent();

    ui.horizontal(|ui| {
        let compact_spacing = f32::from(crate::gui::theme::space::TIGHT);
        ui.spacing_mut().button_padding.x = compact_spacing;
        ui.spacing_mut().item_spacing.x = compact_spacing;

        if crate::gui::widgets::compact_filled_icon_button(
            ui,
            Icon::Priority,
            text.model_catalog.models_hub_button,
            theme.btn_priority(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(text.desktop_settings.usage_statistics_tooltip)
        .clicked()
        {
            *show_models_modal = true;
        }

        if crate::gui::widgets::compact_filled_icon_button(
            ui,
            Icon::Download,
            text.auxiliary.managed_tools.downloaded_tools_button,
            theme.btn_tools(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        {
            *show_tools_modal = true;
        }

        if crate::gui::widgets::compact_filled_icon_button(
            ui,
            Icon::SettingsVoice,
            text.tts_playground.tts_settings_button,
            theme.btn_tts_settings(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        {
            *show_tts_modal = true;
        }

        if crate::gui::widgets::compact_filled_icon_button(
            ui,
            Icon::AutoStories,
            text.shell.help_assistant_btn,
            theme.accent_help(),
            on_btn,
            10,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(text.shell.help_assistant_title)
        .clicked()
        {
            std::thread::spawn(|| {
                crate::gui::settings_ui::help_assistant::show_help_input();
            });
        }
    });

    // === MODELS HUB MODAL (priority / usage / custom tabs) ===
    if render_models_modal(
        ui,
        config,
        usage_stats,
        text,
        ProviderEnabled {
            groq: config.use_groq,
            gemini: config.use_gemini,
            openrouter: config.use_openrouter,
            nvidia: config.use_nvidia,
            ollama: config.use_ollama,
        },
        ModelsHubState {
            show_modal: show_models_modal,
            tab: models_tab,
        },
    ) {
        changed = true;
    }

    // === TOOLS MODAL ===
    let ctx = ui.ctx().clone();
    render_downloaded_tools_modal(&ctx, ui, show_tools_modal, download_manager, text);

    // === TTS SETTINGS MODAL ===
    if render_tts_settings_modal(ui, config, text, show_tts_modal) {
        changed = true;
    }

    if render_restore_defaults_modal(
        ui,
        config,
        text,
        show_restore_defaults_modal,
        run_at_startup,
        startup,
    ) {
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
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Upgrade, Some(crate::gui::icons::ICON_MD));
                ui.vertical(|ui| {
                    render_update_section_content(ui, updater, update_status, text);
                });
            });
        });

    ui.add_space(10.0);

    // === DONATE CARD (Vietnamese-only — supports Vietnamese donors via VietQR) ===
    egui::Frame::new()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            render_donate_section_content(ui, text);
        });

    ui.add_space(10.0);

    // === STARTUP OPTIONS CARD ===
    egui::Frame::new()
        .fill(card_bg)
        .stroke(card_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                draw_icon_static(ui, Icon::Settings, Some(crate::gui::icons::ICON_MD));
                ui.label(
                    egui::RichText::new(text.global_settings.startup_display_header)
                        .strong()
                        .size(14.0),
                );
                ui.add_space(12.0);
                if ui
                    .checkbox(
                        &mut config.show_startup_animation,
                        text.global_settings.show_startup_animation,
                    )
                    .changed()
                {
                    changed = true;
                }
            });
            ui.add_space(6.0);

            changed |= startup::render(
                ui,
                config,
                run_at_startup,
                startup,
                current_admin_state,
                text,
            );

            ui.add_space(8.0);

            config.favorite_overlay_opacity =
                crate::config::types::normalize_result_overlay_opacity_percent(
                    config.favorite_overlay_opacity,
                );
            ui.horizontal(|ui| {
                ui.label(text.global_settings.favorite_overlay_opacity_label);
                if ui
                    .add(
                        egui::Slider::new(
                            &mut config.favorite_overlay_opacity,
                            crate::config::types::MIN_RESULT_OVERLAY_OPACITY_PERCENT
                                ..=crate::config::types::MAX_RESULT_OVERLAY_OPACITY_PERCENT,
                        )
                        .suffix("%"),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            ui.add_space(8.0);

            // Force-quit remains available beside the app settings.
            ui.horizontal(|ui| {
                // Force Quit button — amber (less drastic than the red factory reset).
                if crate::gui::widgets::filled_button(
                    ui,
                    text.preset_basics.force_quit,
                    theme.warning_fill(),
                    theme.on_accent(),
                    8,
                )
                .clicked()
                {
                    crate::gui::app::exit_app();
                }

                ui.add_space(10.0);

                // Keep the destructive red entry point while the trailing
                // chevron makes the extra decision step explicit.
                if crate::gui::widgets::filled_trailing_icon_button(
                    ui,
                    text.preset_basics.reset_defaults_btn,
                    Icon::ArrowRight,
                    theme.danger_fill(),
                    theme.on_accent(),
                    8,
                )
                .clicked()
                {
                    *show_restore_defaults_modal = true;
                }
            });
        });

    ui.add_space(10.0);

    ui.add_space(10.0);

    changed
}
