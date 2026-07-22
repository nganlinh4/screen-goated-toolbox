use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

const MAX_ASSET_BYTES: u64 = 40 * 1024 * 1024;

pub(in crate::overlay::image_to_svg) fn read_asset(path: &str) -> Result<Value, String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES {
        return Err("Preview asset is unavailable or too large.".to_string());
    }
    if path.extension().and_then(|value| value.to_str()) == Some("svg") {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("Could not read vector: {error}"))?;
        return Ok(json!({ "text": text, "sizeBytes": metadata.len() }));
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
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    };
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    Ok(json!({
        "dataUrl": format!("data:{mime};base64,{}", general_purpose::STANDARD.encode(&bytes)),
        "sizeBytes": bytes.len(),
    }))
}

pub(in crate::overlay::image_to_svg) fn save_svg_edits(
    path: &str,
    svg: &str,
) -> Result<Value, String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let is_svg = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("svg"));
    if !metadata.is_file() || !is_svg {
        return Err("Only an existing SVG result can be edited.".to_string());
    }
    if svg.is_empty() || svg.len() as u64 > MAX_ASSET_BYTES {
        return Err("Edited SVG is empty or too large.".to_string());
    }
    let normalized = svg.to_ascii_lowercase();
    if !normalized.contains("<svg")
        || !normalized.contains("</svg>")
        || normalized.contains("<script")
        || normalized.contains("<foreignobject")
        || normalized.contains("javascript:")
        || normalized.contains(" onload=")
        || normalized.contains(" onerror=")
    {
        return Err("Edited SVG contains unsupported active content.".to_string());
    }
    std::fs::write(&path, svg.as_bytes())
        .map_err(|error| format!("Could not save {}: {error}", path.display()))?;
    Ok(json!({ "sizeBytes": svg.len() }))
}

pub(in crate::overlay::image_to_svg) fn open_output(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_valid_svg_edits_and_rejects_active_content() {
        let path = std::env::temp_dir().join(format!(
            "sgt-svg-edit-{}-{}.svg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap();
        let edited = "<svg xmlns=\"http://www.w3.org/2000/svg\"><path fill=\"#123456\" d=\"M0 0h1v1z\"/></svg>";
        assert!(save_svg_edits(path.to_str().unwrap(), edited).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
        assert!(save_svg_edits(path.to_str().unwrap(), "<svg><script/></svg>").is_err());
        let _ = std::fs::remove_file(path);
    }
}
