use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq, Debug)]
pub enum InstallStatus {
    Checking,
    Missing,
    Unavailable,
    Downloading(f32), // 0.0 to 1.0
    Extracting,
    Finalizing,
    Installed,
    Error(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum DownloadState {
    Idle,
    Downloading(f32, String),       // Progress, Status message
    Finished(PathBuf, Option<u32>), // File path, reported short-side resolution
    Error(String),                  // Error message
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
    pub selected_format: Option<String>,
    pub download_type: DownloadType,
    pub selected_subtitle: Option<String>,
    pub(super) analysis: super::analysis::SharedAnalysis,
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
            selected_format: None,
            download_type: default_download_type,
            selected_subtitle: None,
            analysis: Arc::new(Mutex::new(super::analysis::AnalysisState::default())),
            last_input_change: 0.0,
            initial_focus_set: false,
            show_error_log: false,
        }
    }
}

impl Drop for DownloadSession {
    fn drop(&mut self) {
        self.analysis
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .invalidate();
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
    pub fn as_str(&self) -> &'static str {
        match self {
            CookieBrowser::None => "None",
            CookieBrowser::Chrome => "Chrome",
            CookieBrowser::Firefox => "Firefox",
            CookieBrowser::Edge => "Edge",
            CookieBrowser::Brave => "Brave",
            CookieBrowser::Opera => "Opera",
            CookieBrowser::Vivaldi => "Vivaldi",
            CookieBrowser::Chromium => "Chromium",
            CookieBrowser::Whale => "Whale",
        }
    }
}

impl fmt::Display for CookieBrowser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
