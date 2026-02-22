use base64::Engine;
use std::fs;

/// Composite a baked bitmap overlay into a buffer using straight alpha.
/// Writes proper RGBA with alpha for use as a GPU overlay texture layer.
/// The shader blends via `mix(scene, overlay, overlay.a)` in linear space.
pub fn composite_overlay_straight_alpha(
    buffer: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    data: &[u8],
    fade_alpha: f64,
) {
    if fade_alpha <= 0.001 || data.is_empty() {
        return;
    }
    let ow = width as usize;
    let oh = height as usize;
    if data.len() < ow * oh * 4 {
        return;
    }
    for row in 0..oh {
        let dst_y = y + row as i32;
        if dst_y < 0 || dst_y >= buf_h as i32 {
            continue;
        }
        for col in 0..ow {
            let dst_x = x + col as i32;
            if dst_x < 0 || dst_x >= buf_w as i32 {
                continue;
            }
            let src_off = (row * ow + col) * 4;
            let src_a = data[src_off + 3] as f64 / 255.0 * fade_alpha;
            if src_a < 0.004 {
                continue;
            }
            let dst_off = (dst_y as usize * buf_w as usize + dst_x as usize) * 4;
            let dst_a = buffer[dst_off + 3] as f64 / 255.0;
            let inv = 1.0 - src_a;
            let out_a = src_a + dst_a * inv;
            if out_a > 0.001 {
                let w_src = src_a / out_a;
                let w_dst = dst_a * inv / out_a;
                buffer[dst_off] =
                    (data[src_off] as f64 * w_src + buffer[dst_off] as f64 * w_dst) as u8;
                buffer[dst_off + 1] =
                    (data[src_off + 1] as f64 * w_src + buffer[dst_off + 1] as f64 * w_dst) as u8;
                buffer[dst_off + 2] =
                    (data[src_off + 2] as f64 * w_src + buffer[dst_off + 2] as f64 * w_dst) as u8;
                buffer[dst_off + 3] = (out_a * 255.0) as u8;
            }
        }
    }
}

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
    target_w: u32,
    target_h: u32,
) -> Result<Vec<u8>, String> {
    let raw = decode_custom_background_bytes(custom_background)?;
    let decoded = image::load_from_memory(&raw)
        .map_err(|e| format!("Failed to decode custom background image: {}", e))?
        .to_rgba8();

    let src_w = decoded.width().max(1);
    let src_h = decoded.height().max(1);
    let scale = (target_w as f64 / src_w as f64).max(target_h as f64 / src_h as f64);
    let scaled_w = ((src_w as f64 * scale).ceil() as u32).max(target_w);
    let scaled_h = ((src_h as f64 * scale).ceil() as u32).max(target_h);
    let resized = image::imageops::resize(
        &decoded,
        scaled_w,
        scaled_h,
        image::imageops::FilterType::Triangle,
    );
    let crop_x = (scaled_w.saturating_sub(target_w)) / 2;
    let crop_y = (scaled_h.saturating_sub(target_h)) / 2;
    let cropped =
        image::imageops::crop_imm(&resized, crop_x, crop_y, target_w, target_h).to_image();
    Ok(cropped.into_raw())
}
