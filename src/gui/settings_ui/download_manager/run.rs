use super::types::{InstallStatus, UpdateStatus};
use super::utils::log;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;

use super::DownloadManager;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub(super) const FFMPEG_DOWNLOAD_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.0-latest-win64-gpl-8.0.zip";
pub(super) const FFMPEG_RELEASE_API_URL: &str =
    "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest";
pub(super) const FFMPEG_RELEASE_MARKER_FILE: &str = "ffmpeg_release_source.txt";
pub(super) const YTDLP_RELEASE_PAGE_URL: &str =
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest";
pub(super) const YTDLP_RELEASE_API_URL: &str =
    "https://api.github.com/repos/yt-dlp/yt-dlp-nightly-builds/releases/latest";
pub(super) const YTDLP_DOWNLOAD_URL: &str =
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp.exe";
pub(super) const DENO_RELEASE_API_URL: &str =
    "https://api.github.com/repos/denoland/deno/releases/latest";
pub(super) const DENO_DOWNLOAD_URL: &str =
    "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";

pub(super) fn extract_json_string_field(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let pos = json.find(&needle)?;
    let sub = &json[pos..];
    let colon = sub.find(':')?;
    let after_colon = &sub[colon + 1..];
    let quote1 = after_colon.find('"')?;
    let value_start = quote1 + 1;
    let quote2 = after_colon[value_start..].find('"')?;
    Some(after_colon[value_start..value_start + quote2].to_string())
}

pub(super) fn fetch_btbn_release_label() -> Result<String, String> {
    let response = ureq::get(FFMPEG_RELEASE_API_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|e| e.to_string())?;
    let json_str = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;

    // Prefer human-friendly release name, fallback to published timestamp.
    if let Some(name) = extract_json_string_field(&json_str, "name")
        && !name.trim().is_empty()
    {
        return Ok(name);
    }
    if let Some(published_at) = extract_json_string_field(&json_str, "published_at")
        && !published_at.trim().is_empty()
    {
        return Ok(published_at);
    }

    Err("Could not parse BtbN latest release metadata".to_string())
}

pub(super) fn parse_ffmpeg_version(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 3 && parts[0] == "ffmpeg" && parts[1] == "version" {
        Some(parts[2].to_string())
    } else {
        None
    }
}

pub(super) fn fetch_latest_ytdlp_version() -> Result<String, String> {
    let _ = ureq::get(YTDLP_RELEASE_PAGE_URL)
        .header("User-Agent", "Mozilla/5.0")
        .call();

    let response = ureq::get(YTDLP_RELEASE_API_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|e| e.to_string())?;
    let json_str = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;

    extract_json_string_field(&json_str, "tag_name")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "Could not parse latest yt-dlp tag_name".to_string())
}

pub(super) fn read_local_ytdlp_version(ytdlp_path: &PathBuf) -> Result<String, String> {
    let mut cmd = std::process::Command::new(ytdlp_path);
    cmd.arg("--version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("yt-dlp --version failed: {}", output.status));
    }

    let local_ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if local_ver.is_empty() {
        return Err("yt-dlp --version returned empty output".to_string());
    }
    Ok(local_ver)
}

pub(super) fn normalize_semver_version(ver: &str) -> String {
    ver.trim().trim_start_matches('v').to_string()
}

pub(super) fn fetch_latest_deno_version() -> Result<String, String> {
    let response = ureq::get(DENO_RELEASE_API_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|e| e.to_string())?;
    let json_str = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;

    let tag = extract_json_string_field(&json_str, "tag_name")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "Could not parse latest Deno tag_name".to_string())?;
    Ok(normalize_semver_version(&tag))
}

pub(super) fn read_local_deno_version(deno_path: &PathBuf) -> Result<String, String> {
    let mut cmd = std::process::Command::new(deno_path);
    cmd.arg("--version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("deno --version failed: {}", output.status));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout
        .lines()
        .next()
        .ok_or_else(|| "deno --version returned empty output".to_string())?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() >= 2 && parts[0].eq_ignore_ascii_case("deno") {
        let version = normalize_semver_version(parts[1]);
        if version.is_empty() {
            return Err("Could not parse Deno version".to_string());
        }
        Ok(version)
    } else {
        Err("Could not parse Deno version line".to_string())
    }
}

impl DownloadManager {
    pub fn check_status(&self) {
        let bin = self.bin_dir.clone();
        let ffmpeg_s = self.ffmpeg_status.clone();
        let ytdlp_s = self.ytdlp_status.clone();
        let deno_s = self.deno_status.clone();
        let logs = self.install_logs.clone();

        thread::spawn(move || {
            if !bin.exists() {
                let _ = fs::create_dir_all(&bin);
            }

            // Check yt-dlp
            let ytdlp_path = bin.join("yt-dlp.exe");
            if ytdlp_path.exists() {
                *ytdlp_s.lock().unwrap() = InstallStatus::Installed;
            } else {
                *ytdlp_s.lock().unwrap() = InstallStatus::Missing;
                log(&logs, "yt-dlp missing");
            }

            // Check ffmpeg
            let ffmpeg_path = bin.join("ffmpeg.exe");
            let ffprobe_path = bin.join("ffprobe.exe");
            if ffmpeg_path.exists() && ffprobe_path.exists() {
                *ffmpeg_s.lock().unwrap() = InstallStatus::Installed;
            } else {
                *ffmpeg_s.lock().unwrap() = InstallStatus::Missing;
                log(&logs, "ffmpeg missing");
            }

            // Check Deno runtime
            let deno_path = bin.join("deno.exe");
            if deno_path.exists() {
                *deno_s.lock().unwrap() = InstallStatus::Installed;
            } else {
                *deno_s.lock().unwrap() = InstallStatus::Missing;
                log(&logs, "deno runtime missing");
            }

            // Cleanup any partial downloads (.tmp files)
            if let Ok(entries) = fs::read_dir(&bin) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "tmp") {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        });
    }

    pub fn check_updates(&self) {
        if self.is_checking_updates.load(Ordering::Relaxed) {
            return;
        }
        self.is_checking_updates.store(true, Ordering::Relaxed);

        let bin = self.bin_dir.clone();
        let ytdlp_status_store = self.ytdlp_update_status.clone();
        let ffmpeg_status_store = self.ffmpeg_update_status.clone();
        let deno_status_store = self.deno_update_status.clone();
        let ytdlp_ver = self.ytdlp_version.clone();
        let ffmpeg_ver = self.ffmpeg_version.clone();
        let deno_ver = self.deno_version.clone();
        let logs = self.install_logs.clone();
        let ytdlp_install = self.ytdlp_status.clone();
        let ffmpeg_install = self.ffmpeg_status.clone();
        let deno_install = self.deno_status.clone();
        let checking_flag = self.is_checking_updates.clone();

        thread::spawn(move || {
            log(&logs, "Checking for updates...");

            // Set Checking
            *ytdlp_status_store.lock().unwrap() = UpdateStatus::Checking;
            *ffmpeg_status_store.lock().unwrap() = UpdateStatus::Checking;
            *deno_status_store.lock().unwrap() = UpdateStatus::Checking;

            // 1. Check yt-dlp
            // Only if installed
            let mut check_ytdlp = false;
            {
                let s = ytdlp_install.lock().unwrap();
                if *s == InstallStatus::Installed {
                    check_ytdlp = true;
                } else {
                    *ytdlp_status_store.lock().unwrap() = UpdateStatus::Idle;
                }
            }
            if check_ytdlp {
                let ytdlp_path = bin.join("yt-dlp.exe");
                match read_local_ytdlp_version(&ytdlp_path) {
                    Ok(local_ver) => {
                        *ytdlp_ver.lock().unwrap() = Some(local_ver.clone());

                        match fetch_latest_ytdlp_version() {
                            Ok(remote_ver) => {
                                log(
                                    &logs,
                                    format!("yt-dlp: local={}, remote={}", local_ver, remote_ver),
                                );
                                if remote_ver != local_ver {
                                    *ytdlp_status_store.lock().unwrap() =
                                        UpdateStatus::UpdateAvailable(remote_ver);
                                } else {
                                    *ytdlp_status_store.lock().unwrap() = UpdateStatus::UpToDate;
                                }
                            }
                            Err(e) => {
                                *ytdlp_status_store.lock().unwrap() = UpdateStatus::Error(e);
                            }
                        }
                    }
                    Err(e) => {
                        *ytdlp_status_store.lock().unwrap() = UpdateStatus::Error(e);
                    }
                }
            }

            // 2. Check ffmpeg
            let mut check_ffmpeg = false;
            {
                let s = ffmpeg_install.lock().unwrap();
                if *s == InstallStatus::Installed {
                    check_ffmpeg = true;
                } else {
                    *ffmpeg_status_store.lock().unwrap() = UpdateStatus::Idle;
                }
            }
            if check_ffmpeg {
                let ffmpeg_path = bin.join("ffmpeg.exe");
                let output = std::process::Command::new(&ffmpeg_path)
                    .arg("-version")
                    .creation_flags(0x08000000)
                    .output();

                if let Ok(out) = output {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Some(local_ver) = parse_ffmpeg_version(&stdout) {
                        *ffmpeg_ver.lock().unwrap() = Some(local_ver.clone());

                        match fetch_btbn_release_label() {
                            Ok(remote_release) => {
                                let marker_path = bin.join(FFMPEG_RELEASE_MARKER_FILE);
                                let local_release = fs::read_to_string(marker_path)
                                    .ok()
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_default();
                                log(
                                    &logs,
                                    format!(
                                        "ffmpeg: local_ver={}, local_release='{}', remote_release='{}'",
                                        local_ver, local_release, remote_release
                                    ),
                                );
                                if !local_release.is_empty() && local_release == remote_release {
                                    *ffmpeg_status_store.lock().unwrap() = UpdateStatus::UpToDate;
                                } else {
                                    *ffmpeg_status_store.lock().unwrap() =
                                        UpdateStatus::UpdateAvailable(remote_release);
                                }
                            }
                            Err(e) => {
                                *ffmpeg_status_store.lock().unwrap() = UpdateStatus::Error(e);
                            }
                        }
                    }
                }
            }

            // 3. Check Deno
            let mut check_deno = false;
            {
                let s = deno_install.lock().unwrap();
                if *s == InstallStatus::Installed {
                    check_deno = true;
                } else {
                    *deno_status_store.lock().unwrap() = UpdateStatus::Idle;
                }
            }
            if check_deno {
                let deno_path = bin.join("deno.exe");
                match read_local_deno_version(&deno_path) {
                    Ok(local_ver) => {
                        *deno_ver.lock().unwrap() = Some(local_ver.clone());
                        match fetch_latest_deno_version() {
                            Ok(remote_ver) => {
                                log(
                                    &logs,
                                    format!("deno: local={}, remote={}", local_ver, remote_ver),
                                );
                                if remote_ver != local_ver {
                                    *deno_status_store.lock().unwrap() =
                                        UpdateStatus::UpdateAvailable(remote_ver);
                                } else {
                                    *deno_status_store.lock().unwrap() = UpdateStatus::UpToDate;
                                }
                            }
                            Err(e) => {
                                *deno_status_store.lock().unwrap() = UpdateStatus::Error(e);
                            }
                        }
                    }
                    Err(e) => {
                        *deno_status_store.lock().unwrap() = UpdateStatus::Error(e);
                    }
                }
            }
            checking_flag.store(false, Ordering::Relaxed);
            log(&logs, "Update check complete.");
        });
    }

    pub fn get_dependency_sizes(&self) -> (String, String, String) {
        let ytdlp_path = self.bin_dir.join("yt-dlp.exe");
        let ffmpeg_path = self.bin_dir.join("ffmpeg.exe");
        let ffprobe_path = self.bin_dir.join("ffprobe.exe");
        let deno_path = self.bin_dir.join("deno.exe");

        let size_to_string = |path: PathBuf| -> String {
            if let Ok(metadata) = fs::metadata(path) {
                let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;
                format!("{:.1} MB", size_mb)
            } else {
                "0 MB".to_string()
            }
        };

        (
            size_to_string(ytdlp_path),
            {
                let total = [ffmpeg_path, ffprobe_path]
                    .into_iter()
                    .filter_map(|path| fs::metadata(path).ok())
                    .map(|metadata| metadata.len())
                    .sum::<u64>();
                let size_mb = total as f64 / 1024.0 / 1024.0;
                format!("{:.1} MB", size_mb)
            },
            size_to_string(deno_path),
        )
    }

    pub fn delete_dependencies(&self) {
        let ytdlp_path = self.bin_dir.join("yt-dlp.exe");
        let ffmpeg_path = self.bin_dir.join("ffmpeg.exe");
        let ffprobe_path = self.bin_dir.join("ffprobe.exe");
        let ffmpeg_marker_path = self.bin_dir.join(FFMPEG_RELEASE_MARKER_FILE);
        let deno_path = self.bin_dir.join("deno.exe");

        let _ = fs::remove_file(ytdlp_path);
        let _ = fs::remove_file(ffmpeg_path);
        let _ = fs::remove_file(ffprobe_path);
        let _ = fs::remove_file(ffmpeg_marker_path);
        let _ = fs::remove_file(deno_path);

        // Reset status
        *self.ytdlp_status.lock().unwrap() = InstallStatus::Missing;
        *self.ffmpeg_status.lock().unwrap() = InstallStatus::Missing;
        *self.deno_status.lock().unwrap() = InstallStatus::Missing;
    }

    pub fn cancel_download(&self) {
        let idx = self.active_idx();
        if let Some(s) = self.sessions.get(idx) {
            s.cancel_flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn change_download_folder(&mut self) {
        // PowerShell hack to open folder picker
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-Command", "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.ShowDialog() | Out-Null; $f.SelectedPath"]);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);

        let output = cmd.output();

        if let Ok(out) = output
            && let Ok(path) = String::from_utf8(out.stdout)
        {
            let path = path.trim().to_string();
            if !path.is_empty() {
                self.custom_download_path = Some(PathBuf::from(path));
                self.save_settings();
            }
        }
    }
}
