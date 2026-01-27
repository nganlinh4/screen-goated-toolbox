use crate::gui::settings_ui::download_manager::types::{CookieBrowser, DownloadType};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadManagerConfig {
    pub custom_download_path: Option<PathBuf>,
    pub use_metadata: bool,
    pub use_sponsorblock: bool,
    pub use_subtitles: bool,
    pub use_playlist: bool,
    pub cookie_browser: CookieBrowser,
    pub download_type: DownloadType,
    pub selected_subtitle: Option<String>,
}

impl Default for DownloadManagerConfig {
    fn default() -> Self {
        Self {
            custom_download_path: None,
            use_metadata: true,
            use_sponsorblock: false,
            use_subtitles: false,
            use_playlist: false,
            cookie_browser: CookieBrowser::None,
            download_type: DownloadType::Video,
            selected_subtitle: None,
        }
    }
}

pub fn get_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or(PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("download_manager.json")
}

pub fn load_config() -> DownloadManagerConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    DownloadManagerConfig::default()
}

pub fn save_config(config: &DownloadManagerConfig) {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, content);
    }
}
