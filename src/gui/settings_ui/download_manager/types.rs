use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq, Debug)]
pub enum InstallStatus {
    Checking,
    Missing,
    Downloading(f32), // 0.0 to 1.0
    Extracting,
    Installed,
    Error(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum DownloadState {
    Idle,
    Downloading(f32, String),  // Progress, Status message
    Finished(PathBuf, String), // File Path, Success message
    Error(String),             // Error message
}

#[derive(Clone, PartialEq, Debug)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpdateAvailable(String), // remote_version
    UpToDate,
    Error(String),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum DownloadType {
    Video, // Best video+audio -> mkv/mp4
    Audio, // Audio only -> mp3
}

use serde::{Deserialize, Serialize};

/// Holds all per-download-tab state.
pub struct DownloadSession {
    pub tab_name: String,
    pub input_url: String,
    pub download_state: Arc<Mutex<DownloadState>>,
    pub cancel_flag: Arc<AtomicBool>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub available_formats: Arc<Mutex<Vec<String>>>,
    pub selected_format: Option<String>,
    pub available_subs_manual: Arc<Mutex<Vec<String>>>,
    pub download_type: DownloadType,
    pub selected_subtitle: Option<String>,
    pub is_analyzing: Arc<Mutex<bool>>,
    pub last_url_analyzed: String,
    pub analysis_error: Arc<Mutex<Option<String>>>,
    pub last_input_change: f64,
    pub initial_focus_set: bool,
    pub show_error_log: bool,
}

impl DownloadSession {
    pub fn new(tab_name: impl Into<String>, default_download_type: DownloadType) -> Self {
        Self {
            tab_name: tab_name.into(),
            input_url: String::new(),
            download_state: Arc::new(Mutex::new(DownloadState::Idle)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            logs: Arc::new(Mutex::new(Vec::new())),
            available_formats: Arc::new(Mutex::new(Vec::new())),
            selected_format: None,
            available_subs_manual: Arc::new(Mutex::new(Vec::new())),
            download_type: default_download_type,
            selected_subtitle: None,
            is_analyzing: Arc::new(Mutex::new(false)),
            last_url_analyzed: String::new(),
            analysis_error: Arc::new(Mutex::new(None)),
            last_input_change: 0.0,
            initial_focus_set: false,
            show_error_log: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Eq, Hash, Serialize, Deserialize)]
pub enum CookieBrowser {
    None,
    Chrome,
    Firefox,
    Edge,
    Brave,
    Opera,
    Vivaldi,
    Chromium,
    Whale,
}

impl CookieBrowser {
    pub fn to_string(&self) -> String {
        match self {
            CookieBrowser::None => "None".to_string(),
            CookieBrowser::Chrome => "Chrome".to_string(),
            CookieBrowser::Firefox => "Firefox".to_string(),
            CookieBrowser::Edge => "Edge".to_string(),
            CookieBrowser::Brave => "Brave".to_string(),
            CookieBrowser::Opera => "Opera".to_string(),
            CookieBrowser::Vivaldi => "Vivaldi".to_string(),
            CookieBrowser::Chromium => "Chromium".to_string(),
            CookieBrowser::Whale => "Whale".to_string(),
        }
    }
}
