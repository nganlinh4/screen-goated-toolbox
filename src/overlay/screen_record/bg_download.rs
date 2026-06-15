// --- SCREEN RECORD BACKGROUND DOWNLOAD ---
// Downloadable background image support with per-item progress tracking.

use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{Read as IoRead, Write};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::thread;

mod resolver;

use resolver::{detect_download_ext, resolve_image_url};

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
                    vec![
                        DownloadableBackground {
                            id: "warm-abstract".to_string(),
                            download_url: "https://photos.google.com/share/AF1QipNNQyeVrqxBdNmBkq9ILswizuj-RYJFNt5GlxJZ90Y6hx0okrVSLKSnmFFbX7j5Mg/photo/AF1QipPN4cVT1Rngl_wMHjLy1uWx0aiSyENSm8GWW3Ez?key=RV8tSXVJVGdfS1RIQUI0Q3RZZVhlTmw0WmhFZ2V3".to_string(),
                        },
                        DownloadableBackground {
                            id: "cool-abstract".to_string(),
                            download_url: "https://photos.google.com/share/AF1QipNNQyeVrqxBdNmBkq9ILswizuj-RYJFNt5GlxJZ90Y6hx0okrVSLKSnmFFbX7j5Mg/photo/AF1QipNUuKkC-kKZKGQjJ7ga59EJY1d4YwYp0HVeuJ0L?key=RV8tSXVJVGdfS1RIQUI0Q3RZZVhlTmw0WmhFZ2V3".to_string(),
                        },
                        DownloadableBackground {
                            id: "deep-abstract".to_string(),
                            download_url: "https://photos.google.com/share/AF1QipNNQyeVrqxBdNmBkq9ILswizuj-RYJFNt5GlxJZ90Y6hx0okrVSLKSnmFFbX7j5Mg/photo/AF1QipPufDAGMvOMDpTHKG574-ERmZxQN-CtcUCYnzKF?key=RV8tSXVJVGdfS1RIQUI0Q3RZZVhlTmw0WmhFZ2V3".to_string(),
                        },
                        DownloadableBackground {
                            id: "vivid-abstract".to_string(),
                            download_url: "https://drive.google.com/file/d/1kYsxUons_HfjMVxeFU4Rkyw27gK83IVv/view?usp=sharing".to_string(),
                        },
                    ]
                })
        })
        .as_slice()
}

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

pub fn start_download_all_missing() -> usize {
    let mut started = 0usize;
    for bg in downloadable_backgrounds() {
        if download_info(&bg.id).is_none() {
            start_download(bg.id.clone(), bg.download_url.clone());
            started += 1;
        }
    }
    started
}

pub fn delete_all_downloaded() -> usize {
    let mut deleted = 0usize;
    for bg in downloadable_backgrounds() {
        if download_info(&bg.id).is_some() {
            delete_downloaded(&bg.id);
            deleted += 1;
        }
    }
    deleted
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

fn backgrounds_dir() -> PathBuf {
    crate::paths::app_local_data_dir().join("backgrounds")
}

fn delete_existing_files(id: &str) {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn is_valid_image_file(path: &std::path::Path, ext: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    if bytes.len() < 12 {
        return false;
    }
    match ext {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8]),
        "webp" => bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP",
        _ => false,
    }
}

pub fn download_info(id: &str) -> Option<(String, u64)> {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            if !is_valid_image_file(&path, ext) {
                let _ = std::fs::remove_file(&path);
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
pub fn delete_downloaded(id: &str) {
    delete_existing_files(id);
    BG_DOWNLOAD_STATUS.lock().unwrap().remove(id);
}

/// Read a downloaded background as a base64 data URL.
pub fn read_as_data_url(id: &str) -> Result<String, String> {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
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
    id: &str,
    file_path: &std::path::Path,
    ext: &str,
) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    let bytes =
        std::fs::read(file_path).map_err(|e| format!("Read downloaded image failed: {e}"))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| format!("Decode downloaded image failed: {e}"))?;
    let (w, h) = (decoded.width().max(1), decoded.height().max(1));
    const MAX_DIM: u32 = 2560;
    if w <= MAX_DIM && h <= MAX_DIM {
        return Ok(());
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

    let final_path = if ext == "jpg" || ext == "jpeg" {
        file_path.to_path_buf()
    } else {
        let dir = file_path
            .parent()
            .ok_or_else(|| "Missing parent dir".to_string())?;
        dir.join(format!("{id}.jpg"))
    };

    if final_path != file_path {
        let _ = std::fs::remove_file(&final_path);
    }
    std::fs::write(&final_path, &out)
        .map_err(|e| format!("Write normalized background failed: {e}"))?;
    if final_path != file_path {
        let _ = std::fs::remove_file(file_path);
    }
    println!(
        "[BgDownload] Normalized {} from {}x{} to {}x{} in {:.2}ms",
        id,
        w,
        h,
        out_w,
        out_h,
        t0.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

/// Persist an uploaded custom background data URL to local app data and return
/// a lightweight protocol URL the frontend can store in project state.
pub fn save_uploaded_data_url(data_url: &str) -> Result<String, String> {
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| "Uploaded background must be a data URL".to_string())?;
    let (meta, data) = rest
        .split_once(',')
        .ok_or_else(|| "Invalid uploaded background data URL".to_string())?;
    if !meta.contains(";base64") {
        return Err("Uploaded background data URL must be base64".to_string());
    }

    let mime = meta.split(';').next().unwrap_or("image/png");
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/png" => "png",
        _ => "png",
    };

    let raw = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("Failed to decode uploaded background base64: {e}"))?;

    // Do not decode here. We prewarm immediately after save, and decoding twice makes
    // uploads feel much slower for large images (e.g. 6k+ PNG/JPEG/WebP).

    let dir = backgrounds_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create backgrounds dir: {e}"))?;
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    mime.hash(&mut hasher);
    raw.hash(&mut hasher);
    let hash = hasher.finish();
    let file_name = format!("upload-{hash:016x}.{ext}");
    let file_path = dir.join(&file_name);

    if !file_path.exists() {
        std::fs::write(&file_path, &raw)
            .map_err(|e| format!("Failed to write uploaded background: {e}"))?;
    }

    let version = std::fs::metadata(&file_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(format!("/bg-downloaded/{file_name}?v={version}"))
}

/// Start downloading a background image in a background thread.
pub fn start_download(id: String, url: String) {
    if matches!(
        get_download_status(&id),
        BgDownloadStatus::Downloading { .. }
    ) {
        return;
    }

    set_download_status(&id, BgDownloadStatus::Downloading { progress: 0.0 });

    thread::spawn(move || {
        let dir = backgrounds_dir();
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }
        delete_existing_files(&id);

        // Resolve the actual image URL (handles Google Photos sharing pages)
        let image_url = match resolve_image_url(&url) {
            Ok(u) => u,
            Err(e) => {
                set_download_status(
                    &id,
                    BgDownloadStatus::Error(format!("URL resolve failed: {e}")),
                );
                return;
            }
        };

        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

        match ureq::get(&image_url).header("User-Agent", ua).call() {
            Ok(response) => {
                let content_type = response
                    .headers()
                    .get("Content-Type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("image/png")
                    .to_string();
                let content_disposition = response
                    .headers()
                    .get("Content-Disposition")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                let ext = match detect_download_ext(&content_type, &content_disposition, &image_url)
                {
                    Ok(ext) => ext,
                    Err(e) => {
                        set_download_status(&id, BgDownloadStatus::Error(e));
                        return;
                    }
                };

                let total_size = response
                    .headers()
                    .get("Content-Length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);

                let file_path = dir.join(format!("{id}.{ext}"));
                let mut reader = response.into_body().into_reader();
                let mut file = match std::fs::File::create(&file_path) {
                    Ok(f) => f,
                    Err(e) => {
                        set_download_status(
                            &id,
                            BgDownloadStatus::Error(format!("File create error: {e}")),
                        );
                        return;
                    }
                };

                let mut downloaded: u64 = 0;
                let mut buffer = [0u8; 16384];

                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = file.write_all(&buffer[..n]) {
                                set_download_status(
                                    &id,
                                    BgDownloadStatus::Error(format!("Write error: {e}")),
                                );
                                return;
                            }
                            downloaded += n as u64;
                            if total_size > 0 {
                                let progress = (downloaded as f32 / total_size as f32) * 100.0;
                                set_download_status(
                                    &id,
                                    BgDownloadStatus::Downloading { progress },
                                );
                            }
                        }
                        Err(e) => {
                            set_download_status(
                                &id,
                                BgDownloadStatus::Error(format!("Read error: {e}")),
                            );
                            return;
                        }
                    }
                }

                let _ = file.sync_all();
                if let Err(e) = normalize_downloaded_image_for_export(&id, &file_path, ext) {
                    set_download_status(
                        &id,
                        BgDownloadStatus::Error(format!("Normalize error: {e}")),
                    );
                    return;
                }
                set_download_status(&id, BgDownloadStatus::Done);
            }
            Err(e) => {
                set_download_status(&id, BgDownloadStatus::Error(e.to_string()));
            }
        }
    });
}
