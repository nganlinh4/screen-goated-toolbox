use super::types::InstallStatus;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn log(logs: &Arc<Mutex<Vec<String>>>, msg: impl Into<String>) {
    logs.lock().unwrap().push(msg.into());
}

pub fn download_file(
    url: &str,
    path: &PathBuf,
    status: &Arc<Mutex<InstallStatus>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;

    let total_size = resp
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Download to temp file first
    let temp_path = path.with_extension("tmp");
    let mut reader = resp.into_body().into_reader();
    let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;

    let mut buffer = [0; 8192];
    let mut downloaded: u64 = 0;

    loop {
        if cancel.load(Ordering::Relaxed) {
            // Cleanup temp file on cancel
            drop(file);
            let _ = fs::remove_file(&temp_path);
            return Err("Cancelled".to_string());
        }
        let bytes_read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])
            .map_err(|e| e.to_string())?;
        downloaded += bytes_read as u64;

        if total_size > 0 {
            let progress = downloaded as f32 / total_size as f32;
            *status.lock().unwrap() = InstallStatus::Downloading(progress);
        }
    }

    // Ensure file is flushed and closed before rename
    drop(file);

    // Rename temp file to final path
    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to rename temp file: {}", e)
    })?;

    Ok(())
}

pub fn extract_ffmpeg(zip_path: &PathBuf, bin_dir: &PathBuf) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name();

        // We only care about ffmpeg.exe
        if name.ends_with("ffmpeg.exe") {
            let mut out_file =
                fs::File::create(bin_dir.join("ffmpeg.exe")).map_err(|e| e.to_string())?;
            io::copy(&mut file, &mut out_file).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    Err("ffmpeg.exe not found in archive".to_string())
}

use super::types::CookieBrowser;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::process::Command;

pub fn fetch_video_formats(
    url: &str,
    bin_dir: &PathBuf,
    cookie_browser: CookieBrowser,
) -> Result<Vec<String>, String> {
    let ytdlp_path = bin_dir.join("yt-dlp.exe");
    if !ytdlp_path.exists() {
        return Err("yt-dlp is missing".to_string());
    }

    let mut args = vec!["--dump-json".to_string(), "--no-playlist".to_string()];

    // Add cookie args
    match cookie_browser {
        CookieBrowser::None => {}
        CookieBrowser::Chrome => {
            args.push("--cookies-from-browser".to_string());
            args.push("chrome".to_string());
        }
        CookieBrowser::Firefox => {
            args.push("--cookies-from-browser".to_string());
            args.push("firefox".to_string());
        }
        CookieBrowser::Edge => {
            args.push("--cookies-from-browser".to_string());
            args.push("edge".to_string());
        }
        CookieBrowser::Brave => {
            args.push("--cookies-from-browser".to_string());
            args.push("brave".to_string());
        }
        CookieBrowser::Opera => {
            args.push("--cookies-from-browser".to_string());
            args.push("opera".to_string());
        }
        CookieBrowser::Vivaldi => {
            args.push("--cookies-from-browser".to_string());
            args.push("vivaldi".to_string());
        }
        CookieBrowser::Chromium => {
            args.push("--cookies-from-browser".to_string());
            args.push("chromium".to_string());
        }
        CookieBrowser::Whale => {
            args.push("--cookies-from-browser".to_string());
            args.push("whale".to_string());
        }
    }

    args.push(url.to_string());

    let mut cmd = Command::new(ytdlp_path);
    cmd.args(&args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output().map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err("Failed to fetch info".to_string());
    }

    let json_str = String::from_utf8_lossy(&output.stdout);

    let mut heights = std::collections::HashSet::new();

    // Robust manual parsing for "height": 123
    let key = "\"height\":";
    for (i, _) in json_str.match_indices(key) {
        let after_key = &json_str[i + key.len()..];
        // Skip whitespace if any
        let num_start_idx = after_key.find(|c: char| !c.is_whitespace()).unwrap_or(0);
        let after_ws = &after_key[num_start_idx..];

        // Find end of number (comma, bracket, brace, whitespace)
        let num_end_idx = after_ws
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_ws.len());

        if num_end_idx > 0 {
            let num_str = &after_ws[..num_end_idx];
            if let Ok(h) = num_str.parse::<u32>() {
                if h > 0 {
                    heights.insert(h);
                }
            }
        }
    }

    if heights.is_empty() {
        // Fallback: Parsing formats field if available or just check if we got valid JSON
        // If empty, maybe try -F?
        // For now, let's return error if truly empty so UI shows it.
        if json_str.len() < 50 {
            return Err(format!("No formats found. Output: {}", json_str)); // Short error
        }
        return Err("No video resolutions found in metadata.".to_string());
    }

    // Convert to Vec strings
    let mut sorted_heights: Vec<u32> = heights.into_iter().collect();
    sorted_heights.sort_unstable_by(|a, b| b.cmp(a)); // Descending

    let mut result = Vec::new();
    for h in sorted_heights {
        result.push(format!("{}p", h));
    }

    Ok(result)
}
