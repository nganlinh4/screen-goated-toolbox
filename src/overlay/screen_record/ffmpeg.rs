// --- SCREEN RECORD FFMPEG ---
// FFmpeg installation, path resolution, and extraction.

use std::io::{Read as IoRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

// FFmpeg installation state
lazy_static::lazy_static! {
    pub static ref FFMPEG_INSTALL_STATUS: Mutex<FfmpegInstallStatus> = Mutex::new(FfmpegInstallStatus::Idle);
}

#[derive(Clone, serde::Serialize)]
pub enum FfmpegInstallStatus {
    Idle,
    Downloading { progress: f32, total_size: u64 },
    Extracting,
    Installed,
    Error(String),
    Cancelled,
}

pub fn get_ffmpeg_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffmpeg.exe")
}

pub fn get_ffprobe_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffprobe.exe")
}

pub fn get_bin_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
}

pub fn start_ffmpeg_installation() {
    // Check if already downloading
    {
        let status = FFMPEG_INSTALL_STATUS.lock().unwrap();
        if matches!(
            *status,
            FfmpegInstallStatus::Downloading { .. } | FfmpegInstallStatus::Extracting
        ) {
            return;
        }
    }

    *FFMPEG_INSTALL_STATUS.lock().unwrap() = FfmpegInstallStatus::Downloading {
        progress: 0.0,
        total_size: 0,
    };

    thread::spawn(move || {
        let bin_dir = get_bin_dir();
        if !bin_dir.exists() {
            let _ = std::fs::create_dir_all(&bin_dir);
        }

        let url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
        let zip_path = bin_dir.join("ffmpeg.zip");

        // Download with progress
        match ureq::get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .call()
        {
            Ok(response) => {
                let total_size = response
                    .headers()
                    .get("Content-Length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);

                let mut reader = response.into_body().into_reader();
                let mut file = match std::fs::File::create(&zip_path) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = *FFMPEG_INSTALL_STATUS.lock().unwrap() =
                            FfmpegInstallStatus::Error(format!("Failed to create zip file: {}", e));
                        return;
                    }
                };

                let mut downloaded: u64 = 0;
                let mut buffer = [0u8; 16384];

                loop {
                    // Check for cancellation
                    if matches!(
                        *FFMPEG_INSTALL_STATUS.lock().unwrap(),
                        FfmpegInstallStatus::Cancelled
                    ) {
                        drop(file);
                        let _ = std::fs::remove_file(&zip_path);
                        return;
                    }

                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = file.write_all(&buffer[..n]) {
                                *FFMPEG_INSTALL_STATUS.lock().unwrap() =
                                    FfmpegInstallStatus::Error(format!("Write error: {}", e));
                                return;
                            }
                            downloaded += n as u64;

                            // Update progress frequently
                            let mut status = FFMPEG_INSTALL_STATUS.lock().unwrap();
                            if total_size > 0 {
                                let progress = (downloaded as f32 / total_size as f32) * 100.0;
                                *status = FfmpegInstallStatus::Downloading { progress, total_size };
                            } else {
                                // If size is unknown, just show that we are downloading
                                *status =
                                    FfmpegInstallStatus::Downloading { progress: 0.1, total_size: 0 };
                            }
                        }
                        Err(e) => {
                            *FFMPEG_INSTALL_STATUS.lock().unwrap() =
                                FfmpegInstallStatus::Error(format!("Read error: {}", e));
                            return;
                        }
                    }
                }

                // Ensure file is flushed and closed
                let _ = file.sync_all();
                drop(file);

                // Check for cancellation before extracting
                if matches!(
                    *FFMPEG_INSTALL_STATUS.lock().unwrap(),
                    FfmpegInstallStatus::Cancelled
                ) {
                    let _ = std::fs::remove_file(&zip_path);
                    return;
                }

                *FFMPEG_INSTALL_STATUS.lock().unwrap() = FfmpegInstallStatus::Extracting;

                // Extract ffmpeg and ffprobe
                match extract_ffmpeg_zip(&zip_path, &bin_dir) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&zip_path);
                        *FFMPEG_INSTALL_STATUS.lock().unwrap() = FfmpegInstallStatus::Installed;
                    }
                    Err(e) => {
                        *FFMPEG_INSTALL_STATUS.lock().unwrap() = FfmpegInstallStatus::Error(e);
                    }
                }
            }
            Err(e) => {
                *FFMPEG_INSTALL_STATUS.lock().unwrap() = FfmpegInstallStatus::Error(e.to_string());
            }
        }
    });
}

fn extract_ffmpeg_zip(zip_path: &PathBuf, bin_dir: &PathBuf) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut found_ffmpeg = false;
    let mut found_ffprobe = false;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        // Extract ffmpeg.exe
        if name.ends_with("ffmpeg.exe") {
            let mut out =
                std::fs::File::create(bin_dir.join("ffmpeg.exe")).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            found_ffmpeg = true;
        }

        // Extract ffprobe.exe
        if name.ends_with("ffprobe.exe") {
            let mut out =
                std::fs::File::create(bin_dir.join("ffprobe.exe")).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            found_ffprobe = true;
        }

        if found_ffmpeg && found_ffprobe {
            break;
        }
    }

    if !found_ffmpeg {
        return Err("ffmpeg.exe not found in archive".to_string());
    }
    if !found_ffprobe {
        return Err("ffprobe.exe not found in archive".to_string());
    }

    Ok(())
}
