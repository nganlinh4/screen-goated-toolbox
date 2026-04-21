use std::fs;
use std::path::{Path, PathBuf};

use crate::overlay::auto_copy_badge::{NotificationType, show_timed_detailed_notification};

pub fn handle_save_subtitle_srt(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let srt_content = args["srtContent"].as_str().ok_or("Missing srtContent")?;
    let default_file_name = args["defaultFileName"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("subtitles.srt");
    let notification_title = args["notificationTitle"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("SRT saved");

    let saved_path = save_subtitle_srt_file(srt_content, default_file_name, notification_title)?;
    Ok(serde_json::json!({ "savedPath": saved_path }))
}

fn save_subtitle_srt_file(
    srt_content: &str,
    default_file_name: &str,
    notification_title: &str,
) -> Result<String, String> {
    let target_dir = sanitize_dir_path(&super::native_export::get_default_export_dir())?;
    let destination = unique_destination(&target_dir, &ensure_srt_extension(default_file_name));

    fs::write(&destination, srt_content.as_bytes())
        .map_err(|error| format!("Failed to write subtitle file: {}", error))?;

    show_timed_detailed_notification(
        notification_title,
        &target_dir.display().to_string(),
        NotificationType::Success,
        2600,
    );

    Ok(destination.to_string_lossy().to_string())
}

fn sanitize_dir_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Target directory is empty".to_string());
    }

    let dir = PathBuf::from(trimmed);
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "Failed to create target directory {}: {}",
                dir.display(),
                error
            )
        })?;
    }
    if !dir.is_dir() {
        return Err(format!("Target path is not a directory: {}", dir.display()));
    }
    Ok(dir)
}

fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let base = Path::new(file_name);
    let stem = base
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("subtitles");
    let ext = base
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    let mut candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    for index in 1..10_000 {
        let next_name = if ext.is_empty() {
            format!("{} ({})", stem, index)
        } else {
            format!("{} ({}).{}", stem, index, ext)
        };
        candidate = dir.join(next_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    if ext.is_empty() {
        dir.join(format!("{}-copy", stem))
    } else {
        dir.join(format!("{}-copy.{}", stem, ext))
    }
}

fn ensure_srt_extension(file_name: &str) -> String {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return "subtitles.srt".to_string();
    }
    if trimmed.to_ascii_lowercase().ends_with(".srt") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.srt")
    }
}
