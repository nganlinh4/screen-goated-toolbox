use super::run::{
    DENO_DOWNLOAD_URL, FFMPEG_DOWNLOAD_URL, FFMPEG_RELEASE_MARKER_FILE, YTDLP_DOWNLOAD_URL,
    fetch_btbn_release_label, parse_ffmpeg_version, read_local_deno_version,
    read_local_ytdlp_version,
};
use super::types::{InstallStatus, UpdateStatus};
use super::utils::{download_file, extract_deno, extract_ffmpeg, log};
use std::fs;
use std::sync::atomic::Ordering;
use std::thread;

use super::DownloadManager;

impl DownloadManager {
    pub fn start_download_ytdlp(&self) {
        let bin = self.bin_dir.clone();
        let status = self.ytdlp_status.clone();
        let update_status = self.ytdlp_update_status.clone();
        let logs = self.install_logs.clone();
        let cancel = self.install_cancel_flag.clone();
        let bin_clone = bin.clone();
        let ytdlp_ver_store = self.ytdlp_version.clone();

        {
            let mut s = status.lock().unwrap();
            if matches!(
                *s,
                InstallStatus::Downloading(_) | InstallStatus::Extracting
            ) {
                return;
            }
            *s = InstallStatus::Downloading(0.0);
            cancel.store(false, Ordering::Relaxed);
        }

        thread::spawn(move || {
            log(&logs, format!("Starting download: {}", YTDLP_DOWNLOAD_URL));

            let ytdlp_path = bin.join("yt-dlp.exe");
            match download_file(YTDLP_DOWNLOAD_URL, &ytdlp_path, &status, &cancel) {
                Ok(_) => {
                    *status.lock().unwrap() = InstallStatus::Installed;
                    log(&logs, "yt-dlp installed successfully");
                    *update_status.lock().unwrap() = UpdateStatus::Idle;

                    // Update version string locally
                    if let Ok(local_ver) = read_local_ytdlp_version(&bin_clone.join("yt-dlp.exe")) {
                        *ytdlp_ver_store.lock().unwrap() = Some(local_ver);
                    }
                }
                Err(e) => {
                    *status.lock().unwrap() = InstallStatus::Error(e.clone());
                    log(&logs, format!("yt-dlp error: {}", e));
                }
            }
        });
    }

    pub fn start_download_deno(&self) {
        let bin = self.bin_dir.clone();
        let status = self.deno_status.clone();
        let update_status = self.deno_update_status.clone();
        let logs = self.install_logs.clone();
        let cancel = self.install_cancel_flag.clone();
        let deno_ver_store = self.deno_version.clone();

        {
            let mut s = status.lock().unwrap();
            if matches!(
                *s,
                InstallStatus::Downloading(_) | InstallStatus::Extracting
            ) {
                return;
            }
            *s = InstallStatus::Downloading(0.0);
            cancel.store(false, Ordering::Relaxed);
        }

        thread::spawn(move || {
            log(&logs, format!("Starting download: {}", DENO_DOWNLOAD_URL));

            let zip_path = bin.join("deno.zip");
            let deno_path = bin.join("deno.exe");
            match download_file(DENO_DOWNLOAD_URL, &zip_path, &status, &cancel) {
                Ok(_) => {
                    log(&logs, "Deno download complete. Extracting...");
                    *status.lock().unwrap() = InstallStatus::Extracting;

                    if cancel.load(Ordering::Relaxed) {
                        *status.lock().unwrap() = InstallStatus::Error("Cancelled".to_string());
                        return;
                    }

                    match extract_deno(&zip_path, &bin) {
                        Ok(_) => {
                            *status.lock().unwrap() = InstallStatus::Installed;
                            *update_status.lock().unwrap() = UpdateStatus::Idle;
                            let _ = fs::remove_file(zip_path);
                            log(&logs, "Deno runtime installed successfully");

                            if let Ok(local_ver) = read_local_deno_version(&deno_path) {
                                *deno_ver_store.lock().unwrap() = Some(local_ver);
                            }
                        }
                        Err(e) => {
                            *status.lock().unwrap() = InstallStatus::Error(e.clone());
                            *update_status.lock().unwrap() = UpdateStatus::Error(e.clone());
                            log(&logs, format!("Deno extract error: {}", e));
                        }
                    }
                }
                Err(e) => {
                    *status.lock().unwrap() = InstallStatus::Error(e.clone());
                    *update_status.lock().unwrap() = UpdateStatus::Error(e.clone());
                    log(&logs, format!("Deno download error: {}", e));
                }
            }
        });
    }

    pub fn start_download_ffmpeg(&self) {
        let bin = self.bin_dir.clone();
        let status = self.ffmpeg_status.clone();
        let update_status = self.ffmpeg_update_status.clone();
        let logs = self.install_logs.clone();
        let cancel = self.install_cancel_flag.clone();
        let app_bin = bin.clone();
        let ffmpeg_ver_store = self.ffmpeg_version.clone();

        {
            let mut s = status.lock().unwrap();
            if matches!(
                *s,
                InstallStatus::Downloading(_) | InstallStatus::Extracting
            ) {
                return;
            }
            *s = InstallStatus::Downloading(0.0);
            cancel.store(false, Ordering::Relaxed);
        }

        thread::spawn(move || {
            let url = FFMPEG_DOWNLOAD_URL;
            log(&logs, format!("Starting download: {}", url));
            let remote_release = fetch_btbn_release_label().ok();

            let zip_path = bin.join("ffmpeg.zip");
            match download_file(url, &zip_path, &status, &cancel) {
                Ok(_) => {
                    log(&logs, "Download complete. Extracting...");
                    *status.lock().unwrap() = InstallStatus::Extracting;

                    if cancel.load(Ordering::Relaxed) {
                        *status.lock().unwrap() = InstallStatus::Error("Cancelled".to_string());
                        return;
                    }

                    match extract_ffmpeg(&zip_path, &bin) {
                        Ok(_) => {
                            *status.lock().unwrap() = InstallStatus::Installed;
                            log(&logs, "ffmpeg installed successfully");
                            let _ = fs::remove_file(zip_path); // Cleanup
                            *update_status.lock().unwrap() = UpdateStatus::Idle;
                            if let Some(label) = remote_release {
                                let _ = fs::write(bin.join(FFMPEG_RELEASE_MARKER_FILE), label);
                            }

                            // Update version string
                            #[cfg(target_os = "windows")]
                            use std::os::windows::process::CommandExt;
                            let output = std::process::Command::new(app_bin.join("ffmpeg.exe"))
                                .arg("-version")
                                .creation_flags(0x08000000)
                                .output();

                            if let Ok(out) = output {
                                let stdout = String::from_utf8_lossy(&out.stdout);
                                if let Some(local_ver) = parse_ffmpeg_version(&stdout) {
                                    *ffmpeg_ver_store.lock().unwrap() = Some(local_ver);
                                }
                            }
                        }
                        Err(e) => {
                            *status.lock().unwrap() = InstallStatus::Error(e.clone());
                            log(&logs, format!("Extract error: {}", e));
                        }
                    }
                }
                Err(e) => {
                    *status.lock().unwrap() = InstallStatus::Error(e.clone());
                    log(&logs, format!("ffmpeg download error: {}", e));
                }
            }
        });
    }
}
