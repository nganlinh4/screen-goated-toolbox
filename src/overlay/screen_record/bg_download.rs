// --- SCREEN RECORD BACKGROUND DOWNLOAD ---
// Downloadable background image support with per-item progress tracking.

#[cfg(feature = "recorder-worker")]
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
#[cfg(feature = "recorder-worker")]
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
#[cfg(feature = "recorder-worker")]
use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;

mod resolver;
mod security;
mod storage;

use resolver::{detect_download_ext, resolve_image_url};
pub(crate) use security::validate_background_id;
use security::{
    MAX_BACKGROUND_BYTES, get_background_response, validate_download_request,
    validate_download_url, validate_image_bytes,
};
use storage::{
    backgrounds_dir, delete_existing_files, is_valid_image_file, publish_prepared_background,
};

pub static BG_DOWNLOAD_STATUS: LazyLock<Mutex<HashMap<String, BgDownloadStatus>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, serde::Serialize)]
pub enum BgDownloadStatus {
    Idle,
    Downloading { progress: f32 },
    Done,
    Error(String),
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadableBackground {
    pub id: String,
    pub download_url: String,
}

#[derive(Clone, Copy)]
#[cfg(not(feature = "recorder-worker"))]
pub struct DownloadableBackgroundSummary {
    pub downloaded_count: usize,
    pub total_count: usize,
    pub downloading_count: usize,
    pub downloaded_bytes: u64,
}

const DOWNLOADABLE_BACKGROUNDS_MANIFEST: &str =
    include_str!("../../../screen-record/src/config/downloadable-backgrounds.json");

pub fn downloadable_backgrounds() -> &'static [DownloadableBackground] {
    static CACHE: OnceLock<Vec<DownloadableBackground>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            serde_json::from_str::<Vec<DownloadableBackground>>(DOWNLOADABLE_BACKGROUNDS_MANIFEST)
                .unwrap_or_else(|err| {
                    eprintln!(
                        "[screen_record::bg_download] Failed to parse downloadable background manifest: {}",
                        err
                    );
                    Vec::new()
                })
        })
        .as_slice()
}

pub(crate) fn validate_catalog_background_id(id: &str) -> Result<(), String> {
    validate_background_id(id)?;
    if downloadable_backgrounds()
        .iter()
        .any(|entry| entry.id == id)
    {
        Ok(())
    } else {
        Err("Unknown downloadable background".to_string())
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn downloadable_background_summary() -> DownloadableBackgroundSummary {
    let mut downloaded_count = 0usize;
    let mut downloading_count = 0usize;
    let mut downloaded_bytes = 0u64;
    let backgrounds = downloadable_backgrounds();

    for bg in backgrounds {
        if let Some(path) = downloaded_background_file(&bg.id) {
            downloaded_count += 1;
            if let Ok(meta) = std::fs::metadata(path) {
                downloaded_bytes += meta.len();
            }
        }
        if matches!(
            get_download_status(&bg.id),
            BgDownloadStatus::Downloading { .. }
        ) {
            downloading_count += 1;
        }
    }

    DownloadableBackgroundSummary {
        downloaded_count,
        total_count: backgrounds.len(),
        downloading_count,
        downloaded_bytes,
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn downloaded_background_file(id: &str) -> Option<PathBuf> {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

#[cfg(not(feature = "recorder-worker"))]
pub fn start_download_all_missing() -> usize {
    let mut started = 0usize;
    for bg in downloadable_backgrounds() {
        if download_info(&bg.id).is_none()
            && start_download(bg.id.clone(), bg.download_url.clone()).is_ok()
        {
            started += 1;
        }
    }
    started
}

#[cfg(not(feature = "recorder-worker"))]
pub fn delete_all_downloaded() -> Result<usize, String> {
    let mut deleted = 0usize;
    for bg in downloadable_backgrounds() {
        if download_info(&bg.id).is_some() {
            delete_downloaded(&bg.id)?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub fn get_download_status(id: &str) -> BgDownloadStatus {
    BG_DOWNLOAD_STATUS
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .unwrap_or(BgDownloadStatus::Idle)
}

fn set_download_status(id: &str, status: BgDownloadStatus) {
    BG_DOWNLOAD_STATUS
        .lock()
        .unwrap()
        .insert(id.to_string(), status);
}

static DOWNLOAD_NONCE: AtomicU64 = AtomicU64::new(0);

pub fn download_info(id: &str) -> Option<(String, u64)> {
    validate_catalog_background_id(id).ok()?;
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            if !is_valid_image_file(&path, ext) {
                continue;
            }
            let version = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            return Some((ext.to_string(), version));
        }
    }
    None
}

/// Delete a downloaded background file.
pub fn delete_downloaded(id: &str) -> Result<(), String> {
    validate_catalog_background_id(id)?;
    delete_existing_files(id)?;
    BG_DOWNLOAD_STATUS.lock().unwrap().remove(id);
    Ok(())
}

/// Read a downloaded background as a base64 data URL.
#[cfg(feature = "recorder-worker")]
pub fn read_as_data_url(id: &str) -> Result<String, String> {
    validate_catalog_background_id(id)?;
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if is_valid_image_file(&path, ext) {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            let mime = match *ext {
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                _ => "image/png",
            };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            return Ok(format!("data:{mime};base64,{b64}"));
        }
    }
    Err(format!("Background '{id}' not found"))
}

fn normalize_downloaded_image_for_export(
    file_path: &std::path::Path,
    ext: &str,
) -> Result<(PathBuf, String), String> {
    let t0 = std::time::Instant::now();
    let bytes =
        std::fs::read(file_path).map_err(|e| format!("Read downloaded image failed: {e}"))?;
    let (w, h) = validate_image_bytes(&bytes)?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| format!("Decode downloaded image failed: {e}"))?;
    const MAX_DIM: u32 = 2560;
    if w <= MAX_DIM && h <= MAX_DIM {
        return Ok((file_path.to_path_buf(), ext.to_string()));
    }

    let ratio = (MAX_DIM as f32 / w as f32).min(MAX_DIM as f32 / h as f32);
    let out_w = ((w as f32) * ratio).round().max(1.0) as u32;
    let out_h = ((h as f32) * ratio).round().max(1.0) as u32;
    // Triangle is much faster than Lanczos and visually sufficient for abstract backgrounds.
    let resized = decoded.resize(out_w, out_h, FilterType::Triangle).to_rgb8();

    // Re-encode as JPEG for much faster subsequent decode and smaller disk footprint.
    let mut out = Vec::new();
    {
        let mut enc = JpegEncoder::new_with_quality(&mut out, 92);
        enc.encode_image(&image::DynamicImage::ImageRgb8(resized))
            .map_err(|e| format!("JPEG encode normalized background failed: {e}"))?;
    }

    let normalized_path = file_path.with_extension("normalized.part");
    let mut normalized = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&normalized_path)
        .map_err(|e| format!("Create normalized background failed: {e}"))?;
    normalized
        .write_all(&out)
        .and_then(|_| normalized.sync_all())
        .map_err(|e| format!("Write normalized background failed: {e}"))?;
    let _ = std::fs::remove_file(file_path);
    println!(
        "[BgDownload] Normalized {} from {}x{} to {}x{} in {:.2}ms",
        file_path.display(),
        w,
        h,
        out_w,
        out_h,
        t0.elapsed().as_secs_f64() * 1000.0
    );
    Ok((normalized_path, "jpg".to_string()))
}

/// Persist an uploaded custom background data URL to local app data and return
/// a lightweight protocol URL the frontend can store in project state.
#[cfg(feature = "recorder-worker")]
pub fn save_uploaded_data_url(data_url: &str) -> Result<String, String> {
    let max_encoded_len = (MAX_BACKGROUND_BYTES as usize / 3 + 1) * 4;
    if data_url.len() > max_encoded_len + 128 {
        return Err("Uploaded background exceeds the 64 MiB limit".to_string());
    }
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| "Uploaded background must be a data URL".to_string())?;
    let (meta, data) = rest
        .split_once(',')
        .ok_or_else(|| "Invalid uploaded background data URL".to_string())?;
    let normalized_meta = meta.to_ascii_lowercase();
    let (mime, ext) = match normalized_meta.as_str() {
        "image/jpeg;base64" => ("image/jpeg", "jpg"),
        "image/jpg;base64" => ("image/jpg", "jpg"),
        "image/webp;base64" => ("image/webp", "webp"),
        "image/png;base64" => ("image/png", "png"),
        _ => return Err("Uploaded background must be a base64 PNG, JPEG, or WebP".to_string()),
    };
    let raw = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("Failed to decode uploaded background base64: {e}"))?;
    validate_image_bytes(&raw)?;

    let dir = backgrounds_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create backgrounds dir: {e}"))?;
    }

    let mut hasher = Sha256::new();
    hasher.update(mime.as_bytes());
    hasher.update(&raw);
    let file_name = format!("upload-{:x}.{ext}", hasher.finalize());
    let file_path = dir.join(&file_name);

    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file_path)
    {
        Ok(mut file) => file
            .write_all(&raw)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Failed to write uploaded background: {e}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&file_path)
                .map_err(|e| format!("Failed to inspect existing uploaded background: {e}"))?;
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() != raw.len() as u64
            {
                return Err(
                    "Existing uploaded background does not match its content id".to_string()
                );
            }
            let matches_content = std::fs::read(&file_path)
                .map(|existing| existing == raw)
                .unwrap_or(false);
            if !matches_content {
                return Err(
                    "Existing uploaded background does not match its content id".to_string()
                );
            }
        }
        Err(error) => return Err(format!("Failed to create uploaded background: {error}")),
    }

    let version = std::fs::metadata(&file_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(format!("/bg-downloaded/{file_name}?v={version}"))
}

#[cfg(feature = "recorder-worker")]
pub fn reconcile_uploaded_files(retained_urls: &[String]) -> Result<usize, String> {
    if retained_urls.len() > 4_096
        || retained_urls
            .iter()
            .any(|url| url.len() > 32 * 1024 || url.contains('\0'))
    {
        return Err("Retained background list exceeds the supported limit".to_string());
    }
    let retained = retained_urls
        .iter()
        .filter_map(|url| url.split("/bg-downloaded/").nth(1))
        .filter_map(|tail| tail.split(['?', '#']).next())
        .filter(|name| is_uploaded_file_name(name))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let dir = backgrounds_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("Failed to inspect uploaded backgrounds: {error}")),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if retained.contains(name) || !is_uploaded_file_name(name) {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= std::time::Duration::from_secs(60));
        if metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && old_enough
            && std::fs::remove_file(path).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(feature = "recorder-worker")]
fn is_uploaded_file_name(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    let Some(hash) = stem.strip_prefix("upload-") else {
        return false;
    };
    matches!(hash.len(), 16 | 64)
        && hash.bytes().all(|value| value.is_ascii_hexdigit())
        && matches!(extension, "jpg" | "png" | "webp")
}

#[cfg(all(test, feature = "recorder-worker"))]
mod uploaded_file_tests {
    use super::is_uploaded_file_name;

    #[test]
    fn cleanup_accepts_only_content_addressed_upload_names() {
        assert!(is_uploaded_file_name("upload-0123456789abcdef.png"));
        assert!(is_uploaded_file_name(
            "upload-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.webp"
        ));
        for unsafe_name in [
            "../upload-0123456789abcdef.png",
            "upload-0123456789abcde.png",
            "upload-0123456789abcdef.svg",
            "upload-0123456789abcdef.png.bak",
            "custom.png",
        ] {
            assert!(!is_uploaded_file_name(unsafe_name));
        }
    }
}

fn download_background(id: &str, url: &str) -> Result<(), String> {
    let dir = backgrounds_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create backgrounds directory: {error}"))?;

    let image_url =
        resolve_image_url(url).map_err(|error| format!("URL resolve failed: {error}"))?;
    validate_download_url(&image_url)?;

    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
              (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    let response = get_background_response(&image_url, ua)?;
    let content_type = response
        .headers()
        .get("Content-Type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let content_disposition = response
        .headers()
        .get("Content-Disposition")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let ext = detect_download_ext(&content_type, &content_disposition, &image_url)?;
    let total_size = response
        .headers()
        .get("Content-Length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if total_size > MAX_BACKGROUND_BYTES {
        return Err("Background image exceeds the 64 MiB limit".to_string());
    }

    let nonce = DOWNLOAD_NONCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = dir.join(format!(
        ".download-{}-{}-{nonce}.part",
        id,
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| format!("Temporary file create error: {error}"))?;
    let transfer = (|| -> Result<(), String> {
        let mut reader = response.into_body().into_reader();
        let mut downloaded = 0u64;
        let mut buffer = [0u8; 16_384];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("Download read error: {error}"))?;
            if count == 0 {
                break;
            }
            downloaded = downloaded
                .checked_add(count as u64)
                .ok_or_else(|| "Background byte count overflowed".to_string())?;
            if downloaded > MAX_BACKGROUND_BYTES {
                return Err("Background image exceeds the 64 MiB limit".to_string());
            }
            file.write_all(&buffer[..count])
                .map_err(|error| format!("Download write error: {error}"))?;
            if total_size > 0 {
                set_download_status(
                    id,
                    BgDownloadStatus::Downloading {
                        progress: (downloaded as f32 / total_size as f32).min(1.0) * 100.0,
                    },
                );
            }
        }
        file.sync_all()
            .map_err(|error| format!("Sync downloaded background failed: {error}"))
    })();
    drop(file);
    if let Err(error) = transfer {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    let (prepared_path, prepared_ext) = match normalize_downloaded_image_for_export(&temp_path, ext)
    {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(format!("Normalize error: {error}"));
        }
    };
    if let Err(error) = publish_prepared_background(id, &prepared_path, &prepared_ext) {
        let _ = std::fs::remove_file(&prepared_path);
        return Err(error);
    }
    Ok(())
}

/// Start downloading an embedded-catalog background in a background thread.
pub fn start_download(id: String, url: String) -> Result<(), String> {
    validate_download_request(&id, &url)?;
    {
        let mut statuses = BG_DOWNLOAD_STATUS.lock().unwrap();
        if matches!(
            statuses.get(&id),
            Some(BgDownloadStatus::Downloading { .. })
        ) {
            return Ok(());
        }
        statuses.insert(id.clone(), BgDownloadStatus::Downloading { progress: 0.0 });
    }

    thread::spawn(move || match download_background(&id, &url) {
        Ok(()) => set_download_status(&id, BgDownloadStatus::Done),
        Err(error) => set_download_status(&id, BgDownloadStatus::Error(error)),
    });
    Ok(())
}

#[cfg(test)]
mod security_tests {
    use super::{validate_background_id, validate_download_request, validate_download_url};

    #[test]
    fn background_ids_are_single_safe_components() {
        assert!(validate_background_id("warm-abstract").is_ok());
        for value in ["../outside", r"..\outside", "C:outside", "a/b"] {
            assert!(validate_background_id(value).is_err());
        }
    }

    #[test]
    fn download_urls_are_https_and_catalog_bound() {
        assert!(validate_download_url("http://photos.google.com/a").is_err());
        assert!(validate_download_url("https://127.0.0.1/a").is_err());
        assert!(validate_download_request("warm-abstract", "https://example.com/a").is_err());
    }
}
