use base64::Engine;
use std::fs;

pub fn decode_custom_background_bytes(custom_background: &str) -> Result<Vec<u8>, String> {
    if let Some(rest) = custom_background.strip_prefix("data:") {
        let (meta, data) = rest
            .split_once(',')
            .ok_or_else(|| "Invalid custom background data URL".to_string())?;
        if !meta.contains(";base64") {
            return Err("Custom background data URL must be base64".to_string());
        }
        return base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("Failed to decode custom background base64: {}", e));
    }

    if let Some(pos) = custom_background.find("/bg-downloaded/") {
        let rel = &custom_background[pos + "/bg-downloaded/".len()..];
        let rel = rel.split(['?', '#']).next().unwrap_or(rel);
        if rel.is_empty() || rel.contains("..") || rel.contains('/') || rel.contains('\\') {
            return Err("Invalid downloadable background path".to_string());
        }
        let file_path = dirs::data_local_dir()
            .ok_or_else(|| "Failed to resolve local app data directory".to_string())?
            .join("screen-goated-toolbox")
            .join("backgrounds")
            .join(rel);
        return fs::read(&file_path).map_err(|e| {
            format!(
                "Failed to read downloadable background {}: {}",
                file_path.display(),
                e
            )
        });
    }

    Err("Unsupported custom background source".to_string())
}

pub fn load_custom_background_rgba(
    custom_background: &str,
) -> Result<(Vec<u8>, u32, u32), String> {
    let raw = decode_custom_background_bytes(custom_background)?;
    let decoded = image::load_from_memory(&raw)
        .map_err(|e| format!("Failed to decode custom background image: {}", e))?
        .to_rgba8();

    let width = decoded.width().max(1);
    let height = decoded.height().max(1);

    // Skip CPU resize/crop — GPU handles object-fit: cover in the shader.
    Ok((decoded.into_raw(), width, height))
}
