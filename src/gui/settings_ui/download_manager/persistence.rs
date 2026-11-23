use crate::gui::settings_ui::download_manager::types::{CookieBrowser, DownloadType};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadManagerConfig {
    pub custom_download_path: Option<PathBuf>,
    pub use_metadata: bool,
    pub use_sponsorblock: bool,
    pub use_subtitles: bool,
    pub use_playlist: bool,
    pub cookie_browser: CookieBrowser,
    pub download_type: DownloadType,
    #[serde(default)]
    pub last_video_format: Option<String>,
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
            last_video_format: Some(super::quality::DEFAULT_VIDEO_QUALITY.to_owned()),
        }
    }
}

pub fn get_config_path() -> PathBuf {
    crate::paths::app_runtime_config_dir().join("download_manager.json")
}

pub fn load_config() -> DownloadManagerConfig {
    let path = get_config_path();
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| legacy_defaults()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            DownloadManagerConfig::default()
        }
        Err(_) => legacy_defaults(),
    }
}

fn legacy_defaults() -> DownloadManagerConfig {
    DownloadManagerConfig {
        last_video_format: None,
        ..DownloadManagerConfig::default()
    }
}

pub(super) fn try_save_config(config: &DownloadManagerConfig) -> std::io::Result<()> {
    write_config(&get_config_path(), config)
}

/// Replaces the settings file atomically: an interrupted save keeps the
/// previous settings instead of leaving a torn file that loads as defaults.
fn write_config(path: &Path, config: &DownloadManagerConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::atomic_json::write_json_atomic(path, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_preference_reads_legacy_settings_and_round_trips_without_files() {
        assert_eq!(
            DownloadManagerConfig::default()
                .last_video_format
                .as_deref(),
            Some("1080p")
        );
        let mut legacy = serde_json::to_value(DownloadManagerConfig::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("last_video_format");
        let mut config: DownloadManagerConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(config.last_video_format, None);
        config.last_video_format = Some("720p".into());
        let saved = serde_json::to_value(&config).unwrap();
        let restored: DownloadManagerConfig = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.last_video_format.as_deref(), Some("720p"));
        config.last_video_format = None;
        let saved = serde_json::to_value(&config).unwrap();
        let restored: DownloadManagerConfig = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.last_video_format, None);
    }

    #[test]
    fn failed_save_keeps_the_previous_settings_file() {
        let root = std::env::temp_dir().join(format!(
            "sgt-download-settings-{}-{}",
            std::process::id(),
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        let path = root.join("download_manager.json");
        let previous = DownloadManagerConfig {
            last_video_format: Some("1080p".into()),
            ..DownloadManagerConfig::default()
        };
        write_config(&path, &previous).unwrap();
        let saved = fs::read(&path).unwrap();

        let result = crate::atomic_json::while_read_only(&path, || {
            write_config(&path, &DownloadManagerConfig::default())
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn per_video_subtitle_choice_is_not_persisted_and_old_files_still_load() {
        let mut legacy = serde_json::to_value(DownloadManagerConfig::default()).unwrap();
        legacy["selected_subtitle"] = "en".into();
        let config: DownloadManagerConfig = serde_json::from_value(legacy).unwrap();
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("selected_subtitle").is_none());
    }
}
