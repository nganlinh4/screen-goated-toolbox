#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod assets;
mod config;
mod debug_log;
pub mod gui;
mod history;
mod hotkey;
mod icon_gen;
mod initialization;
mod model_config;
mod overlay;
mod registry_integration;
mod screen_capture;
mod updater;
pub mod win_types;
mod unpack_dlls;

use config::{load_config, Config, ThemeMode};
use gui::locale::LocaleText;
use history::HistoryManager;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;

pub use hotkey::RESTORE_EVENT;
pub use screen_capture::GdiCapture;

// Window dimensions
pub const WINDOW_WIDTH: f32 = 1230.0;
pub const WINDOW_HEIGHT: f32 = 650.0;

// Wrappers for thread-safe types
use crate::win_types::SendHwnd;

pub struct AppState {
    pub config: Config,
    pub screenshot_handle: Option<GdiCapture>,
    pub hotkeys_updated: bool,
    pub registered_hotkey_ids: Vec<i32>,
    pub model_usage_stats: HashMap<String, String>,
    pub history: Arc<HistoryManager>,
    pub last_active_window: Option<SendHwnd>,
}

lazy_static! {
    pub static ref APP: Arc<Mutex<AppState>> = Arc::new(Mutex::new({
        let config = load_config();
        let history = Arc::new(HistoryManager::new(config.max_history_items));
        AppState {
            config,
            screenshot_handle: None,
            hotkeys_updated: false,
            registered_hotkey_ids: Vec::new(),
            model_usage_stats: HashMap::new(),
            history,
            last_active_window: None,
        }
    }));
}

fn main() -> eframe::Result<()> {
    crate::log_info!("========================================");
    crate::log_info!(
        "Screen Goated Toolbox v{} STARTUP",
        env!("CARGO_PKG_VERSION")
    );
    crate::log_info!("========================================");

    // Unpack embedded DLLs
    unpack_dlls::unpack_dlls();

    // Cleanup temp files
    initialization::cleanup_temporary_files();

    // Ensure context menu entry
    crate::log_info!("Ensuring context menu entry...");
    registry_integration::ensure_context_menu_entry();
    crate::log_info!("Context menu entry ensured.");

    // Initialize COM and DPI
    initialization::init_com_and_dpi();

    // Enable dark mode for native menus
    initialization::enable_dark_mode_for_app();

    // Apply pending updates
    initialization::apply_pending_updates();

    // Set up crash handler
    initialization::setup_crash_handler();

    // Ensure the named event exists
    let _ = RESTORE_EVENT.as_ref();

    // Single instance check
    let _single_instance_mutex = unsafe {
        let instance = CreateMutexW(
            None,
            true,
            w!("Global\\ScreenGoatedToolboxSingleInstanceMutex"),
        );
        if let Ok(handle) = instance {
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // Another instance is running - pass arguments via temp file and signal it
                let args: Vec<String> = std::env::args().collect();
                for arg in args.iter().skip(1) {
                    if arg.starts_with("--") {
                        continue;
                    }
                    let path = std::path::PathBuf::from(arg);
                    if path.exists() && path.is_file() {
                        let temp_file = std::env::temp_dir().join("sgt_pending_file.txt");
                        if let Ok(mut f) = std::fs::File::create(temp_file) {
                            use std::io::Write;
                            let _ = write!(f, "{}", path.to_string_lossy());
                        }
                        break;
                    }
                }

                if let Some(event) = RESTORE_EVENT.as_ref() {
                    let _ = SetEvent(event.0);
                }
                let _ = CloseHandle(handle);
                return Ok(());
            }
            Some(handle)
        } else {
            None
        }
    };

    // Start hotkey listener thread
    std::thread::spawn(|| {
        hotkey::run_hotkey_listener();
    });

    // Initialize TTS
    api::tts::init_tts();

    // Initialize Gemini Live connection pool
    api::gemini_live::init_gemini_live();

    // Check for --restarted flag and file arguments
    let args: Vec<String> = std::env::args().collect();
    let mut pending_file_path: Option<std::path::PathBuf> = None;

    if args.iter().any(|arg| arg == "--restarted") {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            overlay::auto_copy_badge::show_update_notification(
                "Đã khởi động lại app để khôi phục hoàn toàn",
            );
        });
    }

    for arg in args.iter().skip(1) {
        if arg.starts_with("--") {
            continue;
        }
        let path = std::path::PathBuf::from(arg);
        if path.exists() && path.is_file() {
            crate::log_info!("Check arguments: Found valid file path: {:?}", path);
            pending_file_path = Some(path);
            break;
        } else {
            crate::log_info!("Check arguments: Invalid path or not a file: {:?}", arg);
        }
    }

    // Clear WebView data if scheduled
    {
        let mut config = APP.lock().unwrap();
        if config.config.clear_webview_on_startup {
            overlay::clear_webview_permissions();
            config.config.clear_webview_on_startup = false;
            config::save_config(&config.config);
        }
    }

    // Spawn warmup thread
    initialization::spawn_warmup_thread();

    // Load config for tray setup
    let initial_config = APP.lock().unwrap().config.clone();

    // Tray menu setup
    let tray_locale = LocaleText::get(&initial_config.ui_language);
    let tray_menu = Menu::new();

    let has_favorites = initial_config.presets.iter().any(|p| p.is_favorite);
    let favorite_bubble_text = if has_favorites {
        tray_locale.tray_favorite_bubble
    } else {
        tray_locale.tray_favorite_bubble_disabled
    };
    let tray_favorite_bubble_item = CheckMenuItem::with_id(
        "1003",
        favorite_bubble_text,
        has_favorites,
        initial_config.show_favorite_bubble && has_favorites,
        None,
    );

    let tray_settings_item = MenuItem::with_id("1002", tray_locale.tray_settings, true, None);
    let tray_quit_item = MenuItem::with_id("1001", tray_locale.tray_quit, true, None);
    let _ = tray_menu.append(&tray_favorite_bubble_item);
    let _ = tray_menu.append(&tray_settings_item);
    let _ = tray_menu.append(&tray_quit_item);

    // Window setup
    let mut viewport_builder = eframe::egui::ViewportBuilder::default()
        .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
        .with_resizable(true)
        .with_visible(false)
        .with_transparent(true)
        .with_decorations(false);

    // Detect system theme
    let system_dark = gui::utils::is_system_in_dark_mode();

    // Resolve initial theme
    let effective_dark = match initial_config.theme_mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_dark,
    };

    // Set window icon
    let icon_data = crate::icon_gen::get_window_icon(effective_dark);
    viewport_builder = viewport_builder.with_icon(std::sync::Arc::new(icon_data));

    let options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };

    eframe::run_native(
        "Screen Goated Toolbox (SGT by nganlinh4)",
        options,
        Box::new(move |cc| {
            gui::configure_fonts(&cc.egui_ctx);

            // Store global context for background threads
            *gui::GUI_CONTEXT.lock().unwrap() = Some(cc.egui_ctx.clone());

            // Set initial visuals
            if effective_dark {
                cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());
            } else {
                cc.egui_ctx.set_visuals(eframe::egui::Visuals::light());
            }

            // Set native icon
            gui::utils::update_window_icon_native(effective_dark);

            Ok(Box::new(gui::SettingsApp::new(
                initial_config,
                APP.clone(),
                tray_menu,
                tray_settings_item,
                tray_quit_item,
                tray_favorite_bubble_item,
                cc.egui_ctx.clone(),
                pending_file_path,
            )))
        }),
    )
}

// Re-export hotkey functions for external access
pub use hotkey::{register_all_hotkeys, unregister_all_hotkeys};
