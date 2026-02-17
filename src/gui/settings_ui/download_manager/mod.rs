pub mod detection;
pub mod persistence;
pub mod run;
pub mod types;
pub mod ui;
pub mod utils;

pub use self::types::{CookieBrowser, DownloadState, DownloadType, InstallStatus, UpdateStatus};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct DownloadManager {
    pub show_window: bool,
    pub ffmpeg_status: Arc<Mutex<InstallStatus>>,
    pub ytdlp_status: Arc<Mutex<InstallStatus>>,
    pub deno_status: Arc<Mutex<InstallStatus>>,
    pub ffmpeg_update_status: Arc<Mutex<UpdateStatus>>,
    pub ytdlp_update_status: Arc<Mutex<UpdateStatus>>,
    pub deno_update_status: Arc<Mutex<UpdateStatus>>,
    pub ffmpeg_version: Arc<Mutex<Option<String>>>,
    pub ytdlp_version: Arc<Mutex<Option<String>>>,
    pub deno_version: Arc<Mutex<Option<String>>>,
    pub is_checking_updates: Arc<AtomicBool>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub bin_dir: PathBuf,

    // Downloader State
    pub input_url: String,
    pub download_state: Arc<Mutex<DownloadState>>,

    // Config
    pub custom_download_path: Option<PathBuf>,
    pub cancel_flag: Arc<AtomicBool>,

    // Advanced Options
    pub use_metadata: bool,
    pub use_sponsorblock: bool,
    pub use_subtitles: Arc<Mutex<bool>>,
    pub use_playlist: bool,
    pub cookie_browser: CookieBrowser,
    pub available_browsers: Vec<CookieBrowser>,

    // Analysis State
    pub available_formats: Arc<Mutex<Vec<String>>>, // e.g. "1080p", "720p"
    pub selected_format: Option<String>,
    pub available_subs_manual: Arc<Mutex<Vec<String>>>, // From 'subtitles'
    pub download_type: DownloadType,
    pub selected_subtitle: Option<String>,
    pub is_analyzing: Arc<Mutex<bool>>,
    pub last_url_analyzed: String,
    pub analysis_error: Arc<Mutex<Option<String>>>,
    pub last_input_change: f64, // timestamp
    pub initial_focus_set: bool,
    pub show_error_log: bool,
    pub show_cookie_deno_dialog: bool,
    pub pending_cookie_browser: Option<CookieBrowser>,
}

impl DownloadManager {
    pub fn new() -> Self {
        let bin_dir = dirs::data_local_dir()
            .unwrap_or(PathBuf::from("."))
            .join("screen-goated-toolbox")
            .join("bin");

        let available_browsers = detection::detect_installed_browsers();

        // Load Config
        let config = persistence::load_config();

        // Determine initial browser: Config > First Detected > None
        // But only if config browser is still available or None?
        // For simplicity, prefer config. If config is default (None) and we have browsers, maybe default to detected?
        // Actually, load_config() returns Default (None) if file missing.
        // Logic:
        // 1. If config file existed and loaded, respect it (even if strictly None).
        // 2. If config file missing (default), try auto-detect.
        // To implement (2), we check if config path exists *inside* load_config, but here we just get a struct.
        // Let's refine `load_config` or just check: if `cookie_browser` is None, we *might* want to auto-select,
        // UNLESS user explicitly set it to None.
        // But if user explicitly saved "None", how do we know?
        // Maybe just trust config. If it's the first run, persistent file doesn't exist.
        // We can check if file exists in `new`.

        let config_exists = persistence::get_config_path().exists();
        let default_browser = if config_exists {
            config.cookie_browser.clone()
        } else {
            CookieBrowser::None
        };

        let manager = Self {
            show_window: false,
            ffmpeg_status: Arc::new(Mutex::new(InstallStatus::Checking)),
            ytdlp_status: Arc::new(Mutex::new(InstallStatus::Checking)),
            deno_status: Arc::new(Mutex::new(InstallStatus::Checking)),
            ffmpeg_update_status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            ytdlp_update_status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            deno_update_status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            ffmpeg_version: Arc::new(Mutex::new(None)),
            ytdlp_version: Arc::new(Mutex::new(None)),
            deno_version: Arc::new(Mutex::new(None)),
            is_checking_updates: Arc::new(AtomicBool::new(false)),
            logs: Arc::new(Mutex::new(Vec::new())),
            bin_dir: bin_dir.clone(),
            input_url: String::new(),
            download_state: Arc::new(Mutex::new(DownloadState::Idle)),
            custom_download_path: config.custom_download_path,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            use_metadata: config.use_metadata,
            use_sponsorblock: config.use_sponsorblock,
            use_subtitles: Arc::new(Mutex::new(config.use_subtitles)),
            use_playlist: config.use_playlist,
            cookie_browser: default_browser,
            available_browsers,

            available_formats: Arc::new(Mutex::new(Vec::new())),
            selected_format: None,
            available_subs_manual: Arc::new(Mutex::new(Vec::new())),
            download_type: config.download_type,
            selected_subtitle: config.selected_subtitle,
            is_analyzing: Arc::new(Mutex::new(false)),
            last_url_analyzed: String::new(),
            analysis_error: Arc::new(Mutex::new(None)),
            last_input_change: 0.0,
            initial_focus_set: false,
            show_error_log: false,
            show_cookie_deno_dialog: false,
            pending_cookie_browser: None,
        };

        manager.check_status();
        manager
    }

    pub fn save_settings(&self) {
        let config = persistence::DownloadManagerConfig {
            custom_download_path: self.custom_download_path.clone(),
            use_metadata: self.use_metadata,
            use_sponsorblock: self.use_sponsorblock,
            use_subtitles: *self.use_subtitles.lock().unwrap(),
            use_playlist: self.use_playlist,
            cookie_browser: self.cookie_browser.clone(),
            download_type: self.download_type.clone(),
            selected_subtitle: self.selected_subtitle.clone(),
        };
        persistence::save_config(&config);
    }
}
