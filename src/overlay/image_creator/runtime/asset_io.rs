use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

const MAX_ASSET_BYTES: u64 = 60 * 1024 * 1024;

pub(in crate::overlay::image_creator) fn read_asset(path: &str) -> Result<Value, String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES {
        return Err("Preview image is unavailable or too large.".to_string());
    }
    let mime = match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    Ok(json!({
        "dataUrl": format!("data:{mime};base64,{}", general_purpose::STANDARD.encode(&bytes)),
        "sizeBytes": bytes.len(),
    }))
}

pub(in crate::overlay::image_creator) fn open_output(
    requested_path: Option<&str>,
) -> Result<(), String> {
    let path = requested_path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(super::default_output_dir);
    let target = if path.is_file() {
        path.parent()
            .map(PathBuf::from)
            .unwrap_or_else(super::default_output_dir)
    } else {
        path
    };
    open::that(&target).map_err(|error| format!("Could not open {}: {error}", target.display()))
}
