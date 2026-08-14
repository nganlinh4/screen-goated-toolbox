use base64::Engine;
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime};

const MAX_BACKGROUND_BYTES: usize = 64 * 1024 * 1024;
const MAX_BACKGROUND_BASE64_BYTES: usize = MAX_BACKGROUND_BYTES.div_ceil(3) * 4;
const MAX_BACKGROUND_PIXELS: u64 = 67_108_864;

#[derive(Clone)]
struct CachedCustomBackground {
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    file_stamp: Option<FileStamp>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn custom_bg_cache() -> &'static Mutex<HashMap<String, CachedCustomBackground>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedCustomBackground>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_key(url: &str) -> Result<String, String> {
    if url.starts_with("data:") {
        if url.len() > MAX_BACKGROUND_BASE64_BYTES + 64 {
            return Err("Custom background data URL exceeds the 64 MiB limit".to_string());
        }
        let digest = Sha256::digest(url.as_bytes());
        return Ok(format!(
            "data:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
        ));
    }
    Ok(url.split(['?', '#']).next().unwrap_or(url).to_string())
}

fn downloaded_background_path(url: &str) -> Result<PathBuf, String> {
    let pos = url
        .find("/bg-downloaded/")
        .ok_or_else(|| "Unsupported custom background source".to_string())?;
    let rel = &url[pos + "/bg-downloaded/".len()..];
    let rel = rel.split(['?', '#']).next().unwrap_or(rel);
    let (stem, ext) = rel
        .rsplit_once('.')
        .ok_or_else(|| "Downloadable background has no supported extension".to_string())?;
    super::validation::validate_identifier(stem, "background file id")?;
    if !matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp"
    ) {
        return Err("Downloadable background extension is unsupported".to_string());
    }
    let root = dirs::data_local_dir()
        .ok_or_else(|| "Failed to resolve local app data directory".to_string())?
        .join("screen-goated-toolbox")
        .join("backgrounds");
    Ok(root.join(rel))
}

fn file_backed_stamp(url: &str) -> Option<FileStamp> {
    let path = downloaded_background_path(url).ok()?;
    let metadata = fs::metadata(path).ok()?;
    Some(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

pub fn decode_custom_background_bytes(custom_background: &str) -> Result<Vec<u8>, String> {
    if let Some(rest) = custom_background.strip_prefix("data:") {
        let (meta, data) = rest
            .split_once(',')
            .ok_or_else(|| "Invalid custom background data URL".to_string())?;
        if !matches!(
            meta.to_ascii_lowercase().as_str(),
            "image/png;base64" | "image/jpeg;base64" | "image/jpg;base64" | "image/webp;base64"
        ) {
            return Err("Custom background data URL must be a supported base64 image".to_string());
        }
        if data.len() > MAX_BACKGROUND_BASE64_BYTES {
            return Err("Custom background data URL exceeds the 64 MiB limit".to_string());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("Failed to decode custom background base64: {}", e));
        let bytes = bytes?;
        if bytes.len() > MAX_BACKGROUND_BYTES {
            return Err("Custom background exceeds the 64 MiB limit".to_string());
        }
        return Ok(bytes);
    }

    if custom_background.contains("/bg-downloaded/") {
        let file_path = downloaded_background_path(custom_background)?;
        let metadata = fs::symlink_metadata(&file_path).map_err(|error| {
            format!(
                "Failed to inspect background {}: {error}",
                file_path.display()
            )
        })?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_BACKGROUND_BYTES as u64 {
            return Err("Downloadable background is not a bounded regular file".to_string());
        }
        let mut file = fs::File::open(&file_path).map_err(|error| {
            format!("Failed to open background {}: {error}", file_path.display())
        })?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.by_ref()
            .take(MAX_BACKGROUND_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!("Failed to read background {}: {error}", file_path.display())
            })?;
        if bytes.len() > MAX_BACKGROUND_BYTES {
            return Err("Downloadable background exceeds the 64 MiB limit".to_string());
        }
        return Ok(bytes);
    }

    Err("Unsupported custom background source".to_string())
}

pub fn load_custom_background_rgba(
    custom_background: &str,
) -> Result<(Arc<Vec<u8>>, u32, u32), String> {
    let total_start = Instant::now();
    let cache_key = cache_key(custom_background)?;

    if let Some(hit) = custom_bg_cache()
        .lock()
        .map_err(|_| "Custom background cache lock poisoned".to_string())?
        .get(&cache_key)
        .cloned()
    {
        // For file-backed backgrounds, verify the file size hasn't changed
        // (cheap stat check guards against content replacement on disk).
        let stale = match hit.file_stamp {
            Some(cached) => Some(cached) != file_backed_stamp(custom_background),
            None => false,
        };
        if !stale {
            eprintln!(
                "[CustomBg] cache hit: {}x{} rgba={}B in {:.3}ms",
                hit.width,
                hit.height,
                hit.rgba.len(),
                total_start.elapsed().as_secs_f64() * 1000.0
            );
            return Ok((hit.rgba, hit.width, hit.height));
        }
        eprintln!("[CustomBg] cache stale (file size changed), re-decoding");
    }

    let read_start = Instant::now();
    let raw = decode_custom_background_bytes(custom_background)?;
    let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
    let reader = image::ImageReader::new(std::io::Cursor::new(&raw))
        .with_guessed_format()
        .map_err(|error| format!("Failed to detect custom background format: {error}"))?;
    let (source_width, source_height) = reader
        .into_dimensions()
        .map_err(|error| format!("Failed to read custom background dimensions: {error}"))?;
    if source_width == 0
        || source_height == 0
        || u64::from(source_width) * u64::from(source_height) > MAX_BACKGROUND_PIXELS
    {
        return Err("Custom background dimensions exceed the supported limit".to_string());
    }
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
    }

    let rgba = Arc::new(rgba_image.into_raw());

    if let Ok(mut cache) = custom_bg_cache().lock() {
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.insert(
            cache_key,
            CachedCustomBackground {
                rgba: Arc::clone(&rgba),
                width,
                height,
                file_stamp: file_backed_stamp(custom_background),
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
