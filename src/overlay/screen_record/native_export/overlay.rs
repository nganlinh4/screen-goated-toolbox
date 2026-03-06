use base64::Engine;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

#[derive(Clone)]
struct CachedCustomBackground {
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
}

fn custom_bg_cache() -> &'static Mutex<HashMap<String, CachedCustomBackground>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedCustomBackground>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
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
) -> Result<(Arc<Vec<u8>>, u32, u32), String> {
    let total_start = Instant::now();
    if let Some(hit) = custom_bg_cache()
        .lock()
        .map_err(|_| "Custom background cache lock poisoned".to_string())?
        .get(custom_background)
        .cloned()
    {
        eprintln!(
            "[CustomBg] cache hit: {}x{} rgba={}B in {:.3}ms",
            hit.width,
            hit.height,
            hit.rgba.len(),
            total_start.elapsed().as_secs_f64() * 1000.0
        );
        return Ok((hit.rgba, hit.width, hit.height));
    }

    let read_start = Instant::now();
    let raw = decode_custom_background_bytes(custom_background)?;
    let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
    let decode_start = Instant::now();
    let decoded = image::load_from_memory(&raw)
        .map_err(|e| format!("Failed to decode custom background image: {}", e))?;
    let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

    let mut width = decoded.width().max(1);
    let mut height = decoded.height().max(1);
    let mut rgba_image = decoded.to_rgba8();

    const MAX_DIM: u32 = 2560;
    if width > MAX_DIM || height > MAX_DIM {
        let resize_start = Instant::now();
        let ratio = (MAX_DIM as f32 / width as f32).min(MAX_DIM as f32 / height as f32);
        width = ((width as f32) * ratio).round().max(1.0) as u32;
        height = ((height as f32) * ratio).round().max(1.0) as u32;
        rgba_image = image::imageops::resize(
            &rgba_image,
            width,
            height,
            image::imageops::FilterType::Triangle,
        );
        let resize_ms = resize_start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[CustomBg] Downscaled legacy oversized image to {}x{} in {:.2}ms",
            width, height, resize_ms
        );

        // Self-heal on disk for file-backed downloadable/uploaded backgrounds so the
        // expensive resize/decode is only paid once.
        if let Some(pos) = custom_background.find("/bg-downloaded/") {
            let rel = &custom_background[pos + "/bg-downloaded/".len()..];
            let rel = rel.split(['?', '#']).next().unwrap_or(rel);
            if !rel.is_empty()
                && !rel.contains("..")
                && !rel.contains('/')
                && !rel.contains('\\')
                && let Some(dir) = dirs::data_local_dir()
            {
                let file_path = dir
                    .join("screen-goated-toolbox")
                    .join("backgrounds")
                    .join(rel);
                if file_path.exists() {
                    let ext = file_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if ext == "jpg" || ext == "jpeg" {
                        let mut out = Vec::new();
                        let mut enc =
                            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
                        let rgb_image =
                            image::DynamicImage::ImageRgba8(rgba_image.clone()).to_rgb8();
                        if enc
                            .encode_image(&image::DynamicImage::ImageRgb8(rgb_image))
                            .is_ok()
                            && std::fs::write(&file_path, &out).is_ok()
                        {
                            eprintln!("[CustomBg] Self-healed legacy oversized image in-place");
                        }
                    }
                }
            }
        }
    }

    let rgba = Arc::new(rgba_image.into_raw());

    if let Ok(mut cache) = custom_bg_cache().lock() {
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.insert(
            custom_background.to_string(),
            CachedCustomBackground {
                rgba: Arc::clone(&rgba),
                width,
                height,
            },
        );
    }

    eprintln!(
        "[CustomBg] cache miss: src={}B decoded={}x{} rgba={}B read={:.1}ms decode={:.1}ms total={:.1}ms",
        raw.len(),
        width,
        height,
        rgba.len(),
        read_ms,
        decode_ms,
        total_start.elapsed().as_secs_f64() * 1000.0
    );

    // Skip CPU resize/crop — GPU handles object-fit: cover in the shader.
    Ok((rgba, width, height))
}

pub fn prewarm_custom_background(custom_background: &str) -> Result<(), String> {
    let _ = load_custom_background_rgba(custom_background)?;
    Ok(())
}
