pub mod detection;
pub mod ffmpeg_dependency;
pub mod persistence;
pub mod run;
mod run_download;
mod run_install;
pub mod types;
pub mod ui;
pub mod utils;

pub use self::types::{
    CookieBrowser, DownloadSession, DownloadState, DownloadType, InstallStatus, UpdateStatus,
};
use crate::api::realtime_audio::sherpa_onnx::{self, ZipformerLanguage};
use std::collections::HashMap;
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
    pub bin_dir: PathBuf,
    // Logs and cancel for tool installation (ffmpeg/ytdlp/deno), not per-session
    pub install_logs: Arc<Mutex<Vec<String>>>,
    pub install_cancel_flag: Arc<AtomicBool>,

    // Zipformer ASR per-language download state
    pub zipformer_dlls_status: Arc<Mutex<InstallStatus>>,
    pub zipformer_lang_statuses: HashMap<ZipformerLanguage, Arc<Mutex<InstallStatus>>>,

    // Shared download config
    pub custom_download_path: Option<PathBuf>,
    pub use_metadata: bool,
    pub use_sponsorblock: bool,
    pub use_subtitles: Arc<Mutex<bool>>,
    pub use_playlist: bool,
    pub cookie_browser: CookieBrowser,
    pub available_browsers: Vec<CookieBrowser>,

    // Multi-tab sessions
    pub sessions: Vec<DownloadSession>,
    pub active_tab_idx: usize,

    // UI state (window-level, not per-session)
    pub show_cookie_deno_dialog: bool,
    pub pending_cookie_browser: Option<CookieBrowser>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        let bin_dir = dirs::data_local_dir()
            .unwrap_or(PathBuf::from("."))
            .join("screen-goated-toolbox")
            .join("bin");

        let available_browsers = detection::detect_installed_browsers();
        let config = persistence::load_config();

        let config_exists = persistence::get_config_path().exists();
        let default_browser = if config_exists {
            config.cookie_browser.clone()
        } else {
            CookieBrowser::None
        };

        let first_session = DownloadSession::new("Tab 1", config.download_type.clone());

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
            bin_dir,
            install_logs: Arc::new(Mutex::new(Vec::new())),
            install_cancel_flag: Arc::new(AtomicBool::new(false)),
            zipformer_dlls_status: Arc::new(Mutex::new(InstallStatus::Checking)),
            zipformer_lang_statuses: {
                let mut m = HashMap::new();
                for lang in [
                    ZipformerLanguage::English,
                    ZipformerLanguage::Korean,
                    ZipformerLanguage::Chinese,
                    ZipformerLanguage::French,
                    ZipformerLanguage::German,
                    ZipformerLanguage::Spanish,
                    ZipformerLanguage::Russian,
                    ZipformerLanguage::All8Lang,
                ] {
                    m.insert(lang, Arc::new(Mutex::new(InstallStatus::Checking)));
                }
                m
            },
            custom_download_path: config.custom_download_path,
            use_metadata: config.use_metadata,
            use_sponsorblock: config.use_sponsorblock,
            use_subtitles: Arc::new(Mutex::new(config.use_subtitles)),
            use_playlist: config.use_playlist,
            cookie_browser: default_browser,
            available_browsers,
            sessions: vec![first_session],
            active_tab_idx: 0,
            show_cookie_deno_dialog: false,
            pending_cookie_browser: None,
        };

        manager.check_status();
        manager
    }

    /// Returns the index of the active session (clamped to valid range).
    pub fn active_idx(&self) -> usize {
        self.active_tab_idx
            .min(self.sessions.len().saturating_sub(1))
    }

    pub fn add_tab(&mut self) {
        let n = self.sessions.len() + 1;
        let dt = self
            .sessions
            .get(self.active_idx())
            .map(|s| s.download_type.clone())
            .unwrap_or(DownloadType::Video);
        self.sessions
            .push(DownloadSession::new(format!("Tab {}", n), dt));
        self.active_tab_idx = self.sessions.len() - 1;
    }

    pub fn close_tab(&mut self, idx: usize) {
        if self.sessions.len() <= 1 {
            // Don't close the last tab — just clear it instead
            let dt = self.sessions[0].download_type.clone();
            self.sessions[0] = DownloadSession::new("Tab 1", dt);
            return;
        }
        self.sessions.remove(idx);
        // Re-number all tabs
        for (i, s) in self.sessions.iter_mut().enumerate() {
            s.tab_name = format!("Tab {}", i + 1);
        }
        if self.active_tab_idx >= self.sessions.len() {
            self.active_tab_idx = self.sessions.len() - 1;
        }
    }

    pub fn save_settings(&self) {
        let dt = self
            .sessions
            .get(self.active_idx())
            .map(|s| s.download_type.clone())
            .unwrap_or(DownloadType::Video);
        let config = persistence::DownloadManagerConfig {
            custom_download_path: self.custom_download_path.clone(),
            use_metadata: self.use_metadata,
            use_sponsorblock: self.use_sponsorblock,
            use_subtitles: *self.use_subtitles.lock().unwrap(),
            use_playlist: self.use_playlist,
            cookie_browser: self.cookie_browser.clone(),
            download_type: dt,
            selected_subtitle: self
                .sessions
                .get(self.active_idx())
                .and_then(|s| s.selected_subtitle.clone()),
        };
        persistence::save_config(&config);
    }

    pub fn start_download_sherpa_dlls(&self) {
        let status = self.zipformer_dlls_status.clone();
        *status.lock().unwrap() = InstallStatus::Downloading(0.0);
        let stop = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || {
            let status_cb = status.clone();
            let result = sherpa_onnx::dlls::download_sherpa_dlls_with_progress(stop, move |p| {
                *status_cb.lock().unwrap() = InstallStatus::Downloading(p);
            });
            let s = status_after_result(result);
            *status.lock().unwrap() = s;
        });
    }

    pub fn start_download_zipformer_lang(&self, lang: ZipformerLanguage) {
        let status = self.zipformer_lang_statuses[&lang].clone();
        *status.lock().unwrap() = InstallStatus::Downloading(0.0);
        let stop = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || {
            let status_cb = status.clone();
            let result = sherpa_onnx::download_model_with_progress(lang, &stop, move |p| {
                *status_cb.lock().unwrap() = InstallStatus::Downloading(p);
            });
            let s = status_after_result(result);
            *status.lock().unwrap() = s;
        });
    }
}

fn status_after_result(result: anyhow::Result<()>) -> InstallStatus {
    match result {
        Ok(()) => InstallStatus::Installed,
        Err(e) => InstallStatus::Error(e.to_string()),
    }
}
