use std::path::PathBuf;

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
    LibreWolf,
    Waterfox,
    PaleMoon,
    Zen,
    Thorium,
    Arc,
    Floorp,
    Mercury,
    Pulse,
    Comet,
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
            CookieBrowser::LibreWolf => "LibreWolf".to_string(),
            CookieBrowser::Waterfox => "Waterfox".to_string(),
            CookieBrowser::PaleMoon => "Pale Moon".to_string(),
            CookieBrowser::Zen => "Zen Browser".to_string(),
            CookieBrowser::Thorium => "Thorium".to_string(),
            CookieBrowser::Arc => "Arc".to_string(),
            CookieBrowser::Floorp => "Floorp".to_string(),
            CookieBrowser::Mercury => "Mercury".to_string(),
            CookieBrowser::Pulse => "Pulse".to_string(),
            CookieBrowser::Comet => "Comet".to_string(),
        }
    }
}
