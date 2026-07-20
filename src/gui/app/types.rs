use crate::config::{Config, HotkeyConflict};
use crate::gui::settings_ui::ViewMode;
use crate::gui::settings_ui::node_graph::ChainNode;
use crate::updater::{UpdateStatus, Updater};
use auto_launch::AutoLaunch;
use eframe::egui;
use egui_snarl::Snarl;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, LazyLock, Mutex};
use tray_icon::{
    TrayIcon, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
};

pub use crate::hotkey::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

/// A column the detail area can show (right of the preset-controls sidebar).
/// As the window widens, more of these are shown side-by-side instead of being
/// switched via the header tabs (see `SettingsApp::update_detail_layout`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailPane {
    Editor,
    Global,
    History,
}

pub static RESTORE_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));

pub static REQUEST_OPEN_DOWNLOADED_TOOLS: AtomicBool = AtomicBool::new(false);

pub enum UserEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

pub struct SettingsApp {
    pub(crate) config: Config,
    pub(crate) app_state_ref: Arc<Mutex<crate::AppState>>,
    pub(crate) search_query: String,
    pub(crate) tray_icon: Option<TrayIcon>,
    pub(crate) _tray_menu: Menu,

    pub(crate) tray_settings_item: MenuItem, // Store for dynamic i18n update
    pub(crate) tray_quit_item: MenuItem,     // Store for dynamic i18n update
    pub(crate) tray_favorite_bubble_item: CheckMenuItem, // Store for favorite bubble toggle
    pub(crate) last_ui_language: String,     // Track language to detect changes
    pub(crate) tray_retry_timer: f64,        // Timer for lazy tray icon creation
    pub(crate) tray_startup_wait_frames: u16, // Bounds hidden-start wait for tray readiness
    pub(crate) event_rx: Receiver<UserEvent>,
    pub(crate) is_quitting: bool,
    pub(crate) run_at_startup: bool,
    pub(crate) auto_launcher: Option<AutoLaunch>,
    pub(crate) show_api_key: bool,
    pub(crate) show_gemini_api_key: bool,
    pub(crate) show_openrouter_api_key: bool,
    pub(crate) show_cerebras_api_key: bool,
    pub(crate) icon_dark: Option<egui::TextureHandle>,
    pub(crate) icon_light: Option<egui::TextureHandle>,

    pub(crate) view_mode: ViewMode,
    // Responsive detail layout: the panes currently shown as side-by-side columns
    // (recomputed each frame from the window width), and the preset whose editor
    // is the main pane (persists across Global/History focus so it can stay open).
    pub(crate) detail_panes: Vec<DetailPane>,
    pub(crate) current_preset_idx: Option<usize>,
    pub(crate) recording_hotkey_for_preset: Option<usize>,
    pub(crate) hotkey_conflict_msg: Option<HotkeyConflict>,
    pub(crate) recording_sr_hotkey: bool,
    pub(crate) recording_computer_control_hotkey: bool,
    pub(crate) computer_control_hotkey_conflict_msg: Option<HotkeyConflict>,
    pub(crate) splash: Option<crate::gui::splash::SplashScreen>,
    pub(crate) fade_in_start: Option<f64>,

    // 0 = Init/Offscreen, 1 = Early init, 2..34 = Wait, 35 = Create splash,
    // 36 = Show native window, 37 = Re-enable custom chrome, 38 = Ready
    pub(crate) startup_stage: u8,
    pub(crate) custom_chrome_ready: bool,
    pub(crate) custom_chrome_resize_pulse_stage: u8,
    pub(crate) custom_chrome_restore_size: Option<(i32, i32)>,

    pub(crate) cached_audio_devices: Arc<Mutex<Vec<(String, String)>>>,

    pub(crate) updater: Option<Updater>,
    pub(crate) update_rx: Receiver<UpdateStatus>,
    pub(crate) update_status: UpdateStatus,

    // --- NEW FIELDS ---
    pub(crate) current_admin_state: bool, // Track runtime admin status
    pub(crate) last_effective_theme_dark: bool, // Effective dark mode (considering System/Dark/Light)
    pub(crate) last_system_theme_dark: bool,    // Track Windows system theme for icon switching
    pub(crate) theme_check_timer: f64,          // Timer for polling system theme
    // ------------------

    // --- TIP UI STATE ---
    pub(crate) current_tip_idx: usize,
    pub(crate) tip_timer: f64,      // Time when the current cycle started
    pub(crate) tip_fade_state: f32, // 0.0 (Invisible) -> 1.0 (Visible)
    pub(crate) tip_scroll: f32,     // 0.0 (start) -> 1.0 (fully slid left within window)
    pub(crate) show_tips_modal: bool,
    pub(crate) rng_seed: u32,

    // --- NODE GRAPH STATE ---
    pub(crate) snarl: Option<Snarl<ChainNode>>,
    pub(crate) last_edited_preset_key: Option<(usize, String, String)>,
    // ------------------------

    // --- USAGE MODAL STATE ---
    pub(crate) show_usage_modal: bool,
    // --- DROP OVERLAY STATE ---
    pub(crate) drop_overlay_fade: f32,
    // --- TTS SETTINGS MODAL STATE ---
    pub(crate) show_tts_modal: bool,
    pub(crate) show_tools_modal: bool,
    pub(crate) show_model_priority_modal: bool,
    pub(crate) show_custom_models_modal: bool,
    // --------------------

    // --- FAVORITE BUBBLE STATE TRACKING ---
    pub(crate) last_bubble_enabled: bool,
    pub(crate) last_has_favorites: bool,
    // --------------------------------------

    // --- DOWNLOAD MANAGER ---
    pub(crate) download_manager: crate::gui::settings_ui::download_manager::DownloadManager,
    pub(crate) pointer_gallery: crate::gui::settings_ui::pointer_gallery::PointerGallery,
    pub(crate) show_translation_gummy: bool,
    pub(crate) show_tts_playground: bool,
    pub(crate) show_computer_control_dialog: bool,

    // --- ARGUMENT HANDLING ---
    pub(crate) pending_file_path: Option<std::path::PathBuf>,

    // Measured gap rect between left-side buttons and right-side controls.
    // Updated each frame; used as the drag zone for the next frame.
    pub(crate) title_bar_drag_rect: egui::Rect,
    // Set to true once the WM_NCHITTEST resize subclass has been installed.
    pub(crate) resize_subclass_installed: bool,
}
