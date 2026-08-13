use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;

use crate::overlay::auto_copy_badge::{NotificationType, show_timed_detailed_notification};

const PNG_DATA_URL_PREFIX: &str = "data:image/png;base64,";
const MAX_ENCODED_BYTES: usize = 128 * 1024 * 1024;
const MAX_PNG_BYTES: usize = 96 * 1024 * 1024;
const MAX_PNG_AXIS: u32 = 16_384;
const MAX_PNG_PIXELS: u64 = 100_000_000;

pub fn handle_save_current_frame(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let data_url = args["dataUrl"].as_str().ok_or("Missing dataUrl")?;
    let default_file_name = args["defaultFileName"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("recording-frame.png");
    let notification_title = args["notificationTitle"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Frame saved");

    let png = decode_bounded_png_data_url(data_url)?;
    let saved_path = save_png_file(&png, default_file_name, notification_title)?;
    Ok(serde_json::json!({ "savedPath": saved_path }))
}

fn decode_bounded_png_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let encoded = data_url
        .strip_prefix(PNG_DATA_URL_PREFIX)
        .ok_or("Frame must be a PNG data URL")?;
    if encoded.is_empty() || encoded.len() > MAX_ENCODED_BYTES {
        return Err("Encoded frame exceeds the allowed size".to_string());
    }

    let png = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Frame PNG contains invalid base64 data".to_string())?;
    validate_png_header(&png)?;
    Ok(png)
}

fn validate_png_header(png: &[u8]) -> Result<(), String> {
    if png.len() > MAX_PNG_BYTES {
        return Err("Frame PNG exceeds the allowed size".to_string());
    }
    if png.len() < 33 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return Err("Frame data is not a valid PNG".to_string());
    }

    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_PNG_AXIS
        || height > MAX_PNG_AXIS
        || pixels > MAX_PNG_PIXELS
    {
        return Err("Frame PNG dimensions exceed the allowed bounds".to_string());
    }
    Ok(())
}

fn save_png_file(
    png: &[u8],
    default_file_name: &str,
    notification_title: &str,
) -> Result<String, String> {
    let target_dir = PathBuf::from(super::native_export::get_default_export_dir());
    fs::create_dir_all(&target_dir).map_err(|error| {
        format!(
            "Failed to create frame output directory {}: {error}",
            target_dir.display()
        )
    })?;
    if !target_dir.is_dir() {
        return Err(format!(
            "Frame output path is not a directory: {}",
            target_dir.display()
        ));
    }

    let file_name = safe_png_file_name(default_file_name);
    let destination = unique_destination(&target_dir, &file_name);
    fs::write(&destination, png).map_err(|error| format!("Failed to write frame PNG: {error}"))?;

    show_timed_detailed_notification(
        notification_title,
        &target_dir.display().to_string(),
        NotificationType::Success,
        2600,
    );
    Ok(destination.to_string_lossy().to_string())
}

fn safe_png_file_name(value: &str) -> String {
    let stem = Path::new(value.trim())
        .file_name()
        .and_then(|value| Path::new(value).file_stem())
        .and_then(|value| value.to_str())
        .unwrap_or("recording-frame");
    let cleaned: String = stem
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '-'
            } else {
                character
            }
        })
        .take(120)
        .collect();
    let cleaned = cleaned.trim_matches([' ', '.', '-']);
    format!(
        "{}.png",
        if cleaned.is_empty() {
            "recording-frame"
        } else {
            cleaned
        }
    )
}

fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("recording-frame");
    let mut destination = dir.join(file_name);
    for index in 1..10_000 {
        if !destination.exists() {
            return destination;
        }
        destination = dir.join(format!("{stem} ({index}).png"));
    }
    dir.join(format!("{stem}-copy.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        png.extend_from_slice(&width.to_be_bytes());
        png.extend_from_slice(&height.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        png
    }

    #[test]
    fn accepts_bounded_png_canvas_dimensions() {
        assert!(validate_png_header(&png_header(3840, 2160)).is_ok());
    }

    #[test]
    fn rejects_non_png_and_oversized_dimensions() {
        assert!(validate_png_header(b"not a png").is_err());
        assert!(validate_png_header(&png_header(MAX_PNG_AXIS + 1, 1)).is_err());
        assert!(validate_png_header(&png_header(12_000, 12_000)).is_err());
    }

    #[test]
    fn confines_and_sanitizes_suggested_file_names() {
        assert_eq!(
            safe_png_file_name(r#"..\..\My: Project?.jpg"#),
            "My- Project.png"
        );
        assert_eq!(safe_png_file_name("..."), "recording-frame.png");
    }
}
