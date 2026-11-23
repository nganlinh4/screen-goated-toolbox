use super::types::{SettingsApp, UserEvent};
use crate::config::{Config, ThemeMode};
use crate::gui::settings_ui::ViewMode;
use crate::updater::{UpdateStatus, Updater};
use eframe::egui;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

impl SettingsApp {
    pub fn new(init: SettingsAppInit) -> Self {
        let SettingsAppInit {
            mut config,
            app_state,
            ctx,
            pending_file_path,
        } = init;
        if pending_file_path.is_some() {
            super::settings_exit_gate::mark_file_activation();
        }
        // The initial snapshot may predate edits made while the window starts.
        // Apply the marker transition to the current state under the same
        // APP -> save-gate order as every other configuration writer.
        let (config_baseline, show_preset_model_update_modal) =
            if let Ok(mut state) = app_state.lock() {
                let (snapshot, baseline, show) = capture_startup_snapshot(
                    &mut state.config,
                    env!("CARGO_PKG_VERSION"),
                    crate::config::try_save_config,
                );
                config = snapshot;
                (baseline, show)
            } else {
                crate::log_info!("[preset-models] application state unavailable at startup");
                (config.clone(), false)
            };

        let current_admin_state = crate::gui::utils::is_running_as_admin();
        let startup = crate::gui::startup::Startup::new(&config, current_admin_state, &ctx);
        let run_at_startup =
            startup.available() && (config.run_at_startup || config.run_as_admin_on_startup);

        let (tx, rx) = channel();

        // Tray thread
        let tx_tray = tx.clone();
        let ctx_tray = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(event) = TrayIconEvent::receiver().recv() {
                match &event {
                    TrayIconEvent::Click {
                        button: MouseButton::Right,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        // The persistent child has its own repaint target; do
                        // not wake the settings root for tray-only interaction.
                        crate::overlay::tray_popup::show_tray_popup();
                    }
                    _ if tray_event_needs_settings_root(&event) => {
                        let _ = tx_tray.send(UserEvent::Tray(event));
                        ctx_tray.request_repaint();
                    }
                    _ => {}
                }
            }
        });

        let view_mode = if config.presets.is_empty() {
            ViewMode::Global
        } else {
            ViewMode::Preset(if config.active_preset_idx < config.presets.len() {
                config.active_preset_idx
            } else {
                0
            })
        };

        let (up_tx, up_rx) = channel();

        // Detect initial system theme
        let system_dark = crate::gui::utils::is_system_in_dark_mode();

        // Determine effective initial theme
        let effective_dark = match config.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => system_dark,
        };

        let initial_ui_language = config.ui_language.clone(); // Extract before move

        // Capture bubble state before config is moved
        let initial_bubble_enabled = config.show_favorite_bubble;
        let initial_has_favorites = config.presets.iter().any(|p| p.is_favorite);

        // Create tray icon immediately to avoid splash delay
        let taskbar_dark = crate::gui::utils::is_taskbar_in_dark_mode();
        let tray_dark = crate::gui::utils::tray_icon_dark(&config.theme_mode, taskbar_dark);
        let icon = crate::icon_gen::get_tray_icon(tray_dark);
        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("Screen Goated Toolbox (nganlinh4)")
            .with_icon(icon)
            .build()
            .ok();
        if let Some(tray_icon) = &tray_icon {
            super::exit_preparation::register_tray_icon(tray_icon);
        }

        Self {
            config,
            config_baseline,
            config_conflict: None,
            config_save: Default::default(),
            app_state_ref: app_state,
            search_query: String::new(),
            tray_icon, // Created immediately
            last_ui_language: initial_ui_language,
            tray_retry_timer: 0.0,
            tray_startup_wait_frames: 0,
            event_rx: rx,
            run_at_startup,
            startup,
            show_api_key: false,
            show_gemini_api_key: false,
            show_openrouter_api_key: false,
            show_nvidia_api_key: false,
            icon_dark: None,
            icon_light: None,
            current_preset_idx: match view_mode {
                ViewMode::Preset(idx) => Some(idx),
                _ => None,
            },
            view_mode,
            detail_panes: Vec::new(),
            recording_hotkey_for_preset: None,
            hotkey_conflict_msg: None,
            recording_computer_control_hotkey: false,
            computer_control_hotkey_conflict_msg: None,
            recording_screen_translate_hotkey: false,
            recording_screen_translate_fullscreen: false,
            recording_screen_translate_subtitles: false,
            screen_translate_region_edit: None,
            screen_translate_hotkey_conflict_msg: None,
            recording_live_translate_hotkey: false,
            live_translate_hotkey_conflict_msg: None,
            splash: None, // DELAYED CREATION to stage 35 for perfect $t=0$ timing
            startup_stage: 0,
            custom_chrome_ready: false,
            custom_chrome_resize_pulse_stage: 0,
            last_window_geometry: None,
            window_geometry_changed_at: None,
            custom_chrome_restore_size: None,
            snarl: None,
            last_edited_preset_key: None,
            updater: Some(Updater::new(up_tx)),
            update_rx: up_rx,
            update_status: UpdateStatus::Idle,

            // --- NEW FIELD INIT ---
            current_admin_state,
            last_effective_theme_dark: effective_dark,
            last_system_theme_dark: system_dark,
            last_taskbar_dark: taskbar_dark,
            last_tray_icon_dark: tray_dark,
            theme_check_timer: 0.0,
            // ----------------------

            // --- TIP INIT ---
            show_tips_modal: false,
            selected_tips_category: crate::gui::locale::UsageTipCategory::default(),
            // ---------------

            // --- MODELS HUB MODAL INIT ---
            show_models_modal: false,
            models_tab: crate::gui::settings_ui::global::ModelsTab::default(),
            drop_overlay_fade: 0.0,
            // --- TTS SETTINGS MODAL INIT ---
            // --- TTS SETTINGS MODAL INIT ---
            show_tts_modal: false,
            // --- TOOLS MODAL INIT ---
            show_tools_modal: false,
            show_restore_defaults_modal: false,
            show_preset_model_update_modal,
            // -----------------------

            // --- FAVORITE BUBBLE STATE INIT ---
            last_bubble_enabled: initial_bubble_enabled,

            last_has_favorites: initial_has_favorites,
            // ----------------------------------

            // --- DOWNLOAD MANAGER INIT ---
            download_manager: crate::gui::settings_ui::download_manager::DownloadManager::new(),
            pointer_gallery: crate::gui::settings_ui::pointer_gallery::PointerGallery::new(),
            show_translation_gummy: false,
            show_tts_playground: false,
            show_computer_control_dialog: false,
            show_screen_translate_dialog: false,
            show_live_translate_dialog: false,
            // -----------------------------

            // --- ARGUMENT HANDLING ---
            pending_file_path,

            // --- DRAG WARMUP ---
            title_bar_drag_rect: egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(0.0, 0.0),
            ),
            resize_subclass_installed: false,
        }
    }
}

fn capture_startup_snapshot(
    current: &mut Config,
    version: &str,
    save: impl FnOnce(&Config) -> std::io::Result<()>,
) -> (Config, Config, bool) {
    let show = prepare_startup_model_update(current, version, save);
    let baseline = current.clone();
    (baseline.clone(), baseline, show)
}

fn prepare_startup_model_update(
    current: &mut Config,
    version: &str,
    save: impl FnOnce(&Config) -> std::io::Result<()>,
) -> bool {
    let mut candidate = current.clone();
    let before = candidate.pending_preset_model_update.clone();
    let show = match candidate.prepare_preset_model_update_prompt(version) {
        Ok(show) => show,
        Err(error) => {
            crate::log_info!("[preset-models] invalid staged update marker: {error}");
            candidate.pending_preset_model_update = None;
            false
        }
    };
    if before != candidate.pending_preset_model_update {
        if let Err(error) = save(&candidate) {
            crate::log_info!("[preset-models] could not save staged update marker: {error}");
            return false;
        }
        current.pending_preset_model_update = candidate.pending_preset_model_update;
    }
    show
}

pub struct SettingsAppInit {
    pub config: Config,
    pub app_state: Arc<Mutex<crate::AppState>>,
    pub ctx: egui::Context,
    pub pending_file_path: Option<std::path::PathBuf>,
}

/// Only a left double-click restores the window. Hover, move and plain clicks
/// arrive continuously and must not wake the settings root.
fn tray_event_needs_settings_root(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        }
    )
}

#[cfg(test)]
#[path = "init_marker_tests.rs"]
mod init_marker_tests;

#[cfg(test)]
mod tray_event_tests {
    use super::*;
    use tray_icon::TrayIconId;

    #[test]
    fn only_a_left_double_click_wakes_the_settings_root() {
        let id = TrayIconId::new("tray-test");
        let double = |button| TrayIconEvent::DoubleClick {
            id: id.clone(),
            position: Default::default(),
            rect: Default::default(),
            button,
        };
        assert!(tray_event_needs_settings_root(&double(MouseButton::Left)));
        assert!(!tray_event_needs_settings_root(&double(
            MouseButton::Middle
        )));
        for event in [
            TrayIconEvent::Move {
                id: id.clone(),
                position: Default::default(),
                rect: Default::default(),
            },
            TrayIconEvent::Enter {
                id: id.clone(),
                position: Default::default(),
                rect: Default::default(),
            },
            TrayIconEvent::Leave {
                id: id.clone(),
                position: Default::default(),
                rect: Default::default(),
            },
        ] {
            assert!(!tray_event_needs_settings_root(&event));
        }
    }
}
