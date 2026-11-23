use std::fs;

use super::super::raw_video::{sanitize_dir_path, unique_destination};

use crate::overlay::auto_copy_badge::{NotificationType, show_timed_detailed_notification};

const MAX_SRT_BYTES: usize = 64 * 1024 * 1024;

pub fn handle_save_subtitle_srt(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let srt_content = args["srtContent"].as_str().ok_or("Missing srtContent")?;
    if srt_content.len() > MAX_SRT_BYTES {
        return Err("Subtitle export exceeds the 64 MiB limit".to_string());
    }
    let default_file_name = args["defaultFileName"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("subtitles.srt");
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let notification_title = args["notificationTitle"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(badge.subtitles_saved);

    let saved_path = save_subtitle_srt_file(srt_content, default_file_name, notification_title)?;
    Ok(serde_json::json!({ "savedPath": saved_path }))
}

fn save_subtitle_srt_file(
    srt_content: &str,
    default_file_name: &str,
    notification_title: &str,
) -> Result<String, String> {
    let target_dir = sanitize_dir_path(&super::native_export::get_default_export_dir())?;
    let file_name =
        super::path_validation::safe_suggested_file_name(default_file_name, "subtitles", "srt");
    let destination = unique_destination(&target_dir, &file_name);

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

#[cfg(test)]
mod tests {
    use super::super::path_validation::safe_suggested_file_name;

    #[test]
    fn subtitle_suggestions_cannot_escape_the_download_directory() {
        assert_eq!(
            safe_suggested_file_name(r"..\..\captions.srt", "subtitles", "srt"),
            "captions.srt"
        );
    }
}
