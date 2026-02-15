// --- SCREEN RECORD BACKGROUND DOWNLOAD ---
// Downloadable background image support with per-item progress tracking.

use base64::Engine;
use std::collections::HashMap;
use std::io::{Read as IoRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

lazy_static::lazy_static! {
    pub static ref BG_DOWNLOAD_STATUS: Mutex<HashMap<String, BgDownloadStatus>> = Mutex::new(HashMap::new());
}

#[derive(Clone, serde::Serialize)]
pub enum BgDownloadStatus {
    Idle,
    Downloading { progress: f32 },
    Done,
    Error(String),
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
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("backgrounds")
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

/// Check if a background with the given id has been downloaded.
pub fn is_downloaded(id: &str) -> Option<String> {
    let dir = backgrounds_dir();
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("{id}.{ext}"));
        if path.exists() {
            if !is_valid_image_file(&path, ext) {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            return Some(ext.to_string());
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
                set_download_status(&id, BgDownloadStatus::Error(format!("URL resolve failed: {e}")));
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
                        set_download_status(&id, BgDownloadStatus::Error(format!("File create error: {e}")));
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
                                set_download_status(&id, BgDownloadStatus::Error(format!("Write error: {e}")));
                                return;
                            }
                            downloaded += n as u64;
                            if total_size > 0 {
                                let progress = (downloaded as f32 / total_size as f32) * 100.0;
                                set_download_status(&id, BgDownloadStatus::Downloading { progress });
                            }
                        }
                        Err(e) => {
                            set_download_status(&id, BgDownloadStatus::Error(format!("Read error: {e}")));
                            return;
                        }
                    }
                }

                let _ = file.sync_all();
                set_download_status(&id, BgDownloadStatus::Done);
            }
            Err(e) => {
                set_download_status(&id, BgDownloadStatus::Error(e.to_string()));
            }
        }
    });
}

fn extract_filename_ext(content_disposition: &str) -> Option<String> {
    for part in content_disposition.split(';') {
        let p = part.trim();
        if let Some(raw) = p.strip_prefix("filename=") {
            let name = raw.trim_matches('"').trim_matches('\'');
            if let Some(ext) = name.rsplit('.').next() {
                return Some(ext.to_ascii_lowercase());
            }
        }
    }
    None
}

fn detect_download_ext(
    content_type: &str,
    content_disposition: &str,
    url: &str,
) -> Result<&'static str, String> {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("image/png") {
        return Ok("png");
    }
    if ct.contains("image/jpeg") || ct.contains("image/jpg") {
        return Ok("jpg");
    }
    if ct.contains("image/webp") {
        return Ok("webp");
    }
    if ct.contains("text/html") {
        return Err("Downloaded page is HTML, not an image URL".to_string());
    }

    let mut candidate_ext: Option<String> = extract_filename_ext(content_disposition);
    if candidate_ext.is_none() {
        candidate_ext = url
            .rsplit('/')
            .next()
            .and_then(|name| name.rsplit('.').next())
            .map(|s| s.to_ascii_lowercase());
    }

    match candidate_ext.as_deref() {
        Some("png") => Ok("png"),
        Some("jpg") | Some("jpeg") => Ok("jpg"),
        Some("webp") => Ok("webp"),
        Some("heic") | Some("heif") | Some("avif") => Err(
            "Downloaded image format is unsupported (HEIC/HEIF/AVIF). Please use PNG/JPG/WEBP source.".to_string(),
        ),
        Some(other) => Err(format!("Unsupported image format '{}'", other)),
        None => Err(format!(
            "Unsupported content type '{}' (missing image extension)",
            content_type
        )),
    }
}

/// For Google Photos sharing URLs, try multiple strategies to get the actual image URL.
/// For other URLs, return as-is.
fn resolve_image_url(url: &str) -> Result<String, String> {
    // Google Drive: convert /file/d/ID/view → direct download URL
    if url.contains("drive.google.com/file/d/") {
        if let Some(start) = url.find("/file/d/") {
            let after = &url[start + 8..];
            let file_id = after.split('/').next().unwrap_or(after);
            if !file_id.is_empty() {
                return Ok(format!(
                    "https://drive.google.com/uc?export=download&id={file_id}"
                ));
            }
        }
    }

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
