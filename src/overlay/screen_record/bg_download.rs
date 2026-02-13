// --- SCREEN RECORD BACKGROUND DOWNLOAD ---
// Downloadable background image support with progress tracking.

use base64::Engine;
use std::io::{Read as IoRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

lazy_static::lazy_static! {
    pub static ref BG_DOWNLOAD_STATUS: Mutex<BgDownloadStatus> = Mutex::new(BgDownloadStatus::Idle);
}

#[derive(Clone, serde::Serialize)]
pub enum BgDownloadStatus {
    Idle,
    Downloading { progress: f32 },
    Done,
    Error(String),
}

fn backgrounds_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("backgrounds")
}

/// Check if a background with the given id has been downloaded.
pub fn is_downloaded(id: &str) -> Option<String> {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            return Some(ext.to_string());
        }
    }
    None
}

/// Delete a downloaded background file.
pub fn delete_downloaded(id: &str) {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
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

/// Start downloading a background image in a background thread.
pub fn start_download(id: String, url: String) {
    {
        let status = BG_DOWNLOAD_STATUS.lock().unwrap();
        if matches!(*status, BgDownloadStatus::Downloading { .. }) {
            return;
        }
    }

    *BG_DOWNLOAD_STATUS.lock().unwrap() = BgDownloadStatus::Downloading { progress: 0.0 };

    thread::spawn(move || {
        let dir = backgrounds_dir();
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }

        // Resolve the actual image URL (handles Google Photos sharing pages)
        let image_url = match resolve_image_url(&url) {
            Ok(u) => u,
            Err(e) => {
                *BG_DOWNLOAD_STATUS.lock().unwrap() =
                    BgDownloadStatus::Error(format!("URL resolve failed: {e}"));
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

                let ext = if content_type.contains("jpeg") || content_type.contains("jpg") {
                    "jpg"
                } else if content_type.contains("webp") {
                    "webp"
                } else {
                    "png"
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
                        *BG_DOWNLOAD_STATUS.lock().unwrap() =
                            BgDownloadStatus::Error(format!("File create error: {e}"));
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
                                *BG_DOWNLOAD_STATUS.lock().unwrap() =
                                    BgDownloadStatus::Error(format!("Write error: {e}"));
                                return;
                            }
                            downloaded += n as u64;
                            if total_size > 0 {
                                let progress = (downloaded as f32 / total_size as f32) * 100.0;
                                *BG_DOWNLOAD_STATUS.lock().unwrap() =
                                    BgDownloadStatus::Downloading { progress };
                            }
                        }
                        Err(e) => {
                            *BG_DOWNLOAD_STATUS.lock().unwrap() =
                                BgDownloadStatus::Error(format!("Read error: {e}"));
                            return;
                        }
                    }
                }

                let _ = file.sync_all();
                *BG_DOWNLOAD_STATUS.lock().unwrap() = BgDownloadStatus::Done;
            }
            Err(e) => {
                *BG_DOWNLOAD_STATUS.lock().unwrap() =
                    BgDownloadStatus::Error(e.to_string());
            }
        }
    });
}

/// For Google Photos sharing URLs, try multiple strategies to get the actual image URL.
/// For other URLs, return as-is.
fn resolve_image_url(url: &str) -> Result<String, String> {
    if !url.contains("photos.google.com") {
        return Ok(url.to_string());
    }

    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
              (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    // Strategy 1: Fetch the page and look for og:image or lh3 URLs in the HTML/JS
    if let Ok(response) = ureq::get(url).header("User-Agent", ua).call() {
        if let Ok(body) = response.into_body().read_to_string() {
            // Try og:image meta tag
            if let Some(pos) = body.find("og:image") {
                let after = &body[pos..];
                if let Some(c_pos) = after.find("content=\"") {
                    let url_start = c_pos + 9;
                    if let Some(url_end) = after[url_start..].find('"') {
                        let raw_url = &after[url_start..url_start + url_end];
                        let decoded = raw_url.replace("&amp;", "&");
                        let base = decoded.split('=').next().unwrap_or(&decoded);
                        return Ok(format!("{base}=w4096-h4096"));
                    }
                }
            }
            // Try any lh3.googleusercontent.com URL in the page source
            if let Some(pos) = body.find("https://lh3.googleusercontent.com/pw/") {
                if let Some(end) = body[pos..].find(|c: char| c == '"' || c == '\'' || c == '\\') {
                    let raw = &body[pos..pos + end];
                    let decoded = raw.replace("\\u003d", "=").replace("&amp;", "&");
                    let base = decoded.split('=').next().unwrap_or(&decoded);
                    return Ok(format!("{base}=w4096-h4096"));
                }
            }
        }
    }

    // Strategy 2: Extract photo ID from URL path and construct direct lh3 URL
    // URL format: .../photo/AF1Qip.../...
    if let Some(photo_pos) = url.find("/photo/") {
        let after = &url[photo_pos + 7..];
        let photo_id = after.split('?').next().unwrap_or(after);
        if !photo_id.is_empty() {
            let direct = format!(
                "https://lh3.googleusercontent.com/pw/{photo_id}=w4096-h4096"
            );
            // Verify it returns an image (HEAD request)
            if let Ok(resp) = ureq::head(&direct).header("User-Agent", ua).call() {
                let ct = resp
                    .headers()
                    .get("Content-Type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if ct.starts_with("image/") {
                    return Ok(direct);
                }
            }
        }
    }

    Err(
        "Could not resolve a direct image URL from Google Photos. \
         Try using a direct image link instead (e.g. Imgur, Google Drive export, etc.)"
            .to_string(),
    )
}
