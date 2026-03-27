use super::run::{YTDLP_DOWNLOAD_URL, fetch_latest_ytdlp_version, read_local_ytdlp_version};
use super::types::{CookieBrowser, DownloadState, DownloadType, InstallStatus, UpdateStatus};
use super::utils::{download_file, log};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use super::DownloadManager;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

fn set_download_stage(state: &Arc<Mutex<DownloadState>>, msg: impl Into<String>) {
    *state.lock().unwrap() = DownloadState::Downloading(0.0, msg.into());
}

fn set_download_finished(state: &Arc<Mutex<DownloadState>>, final_path: Option<PathBuf>) {
    let path = final_path.unwrap_or_default();
    *state.lock().unwrap() = DownloadState::Finished(path, "Download Completed!".to_string());
}

fn run_ytdlp_download_attempt(
    ytdlp_exe: &PathBuf,
    args: &[String],
    progress_fmt: &str,
    state: &Arc<Mutex<DownloadState>>,
    logs: &Arc<Mutex<Vec<String>>>,
    attempt_label: &str,
) -> Result<Option<PathBuf>, String> {
    use std::process::Stdio;

    let mut cmd = std::process::Command::new(ytdlp_exe);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    log(logs, format!("Running yt-dlp ({})...", attempt_label));

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to open yt-dlp stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to open yt-dlp stderr".to_string())?;

    let logs_clone = logs.clone();
    let state_clone = state.clone();
    let final_filename = Arc::new(Mutex::new(None));
    let final_filename_clone = final_filename.clone();
    let fmt_str = progress_fmt.to_string();

    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for l in reader.lines().map_while(Result::ok) {
            if l.contains("[download]")
                && l.contains("%")
                && let Some(start) = l.find("%")
            {
                let substr = &l[..start];
                if let Some(space) = substr.rfind(' ')
                    && let Ok(p) = substr[space + 1..].parse::<f32>()
                {
                    let parts: Vec<&str> = l.split_whitespace().collect();

                    let mut p_val = None;
                    let mut t_val = None;
                    let mut s_val = None;
                    let mut e_val = None;

                    for (i, part) in parts.iter().enumerate() {
                        if part.contains("%") {
                            p_val = Some(part.trim_end_matches('%'));
                        } else if *part == "of" && i + 1 < parts.len() {
                            let val = parts[i + 1];
                            if val != "Unknown" && val != "N/A" {
                                t_val = Some(val);
                            }
                        } else if *part == "at" && i + 1 < parts.len() {
                            let val = parts[i + 1];
                            if val != "Unknown" && val != "N/A" {
                                s_val = Some(val);
                            }
                        } else if *part == "ETA" && i + 1 < parts.len() {
                            let val = parts[i + 1];
                            if val != "Unknown" && val != "N/A" {
                                e_val = Some(val);
                            }
                        }
                    }

                    let fmt_segments: Vec<&str> = fmt_str.split("{}").collect();
                    let mut status_msg = String::new();

                    if let Some(p_str) = p_val {
                        if fmt_segments.len() >= 5 {
                            status_msg.push_str(fmt_segments[0]);
                            status_msg.push_str(p_str);

                            if let Some(t) = t_val {
                                status_msg.push_str(fmt_segments[1]);
                                status_msg.push_str(t);
                            } else {
                                status_msg.push('%');
                            }

                            if let Some(s) = s_val {
                                status_msg.push_str(fmt_segments[2]);
                                status_msg.push_str(s);
                            }

                            if let Some(e) = e_val {
                                status_msg.push_str(fmt_segments[3]);
                                status_msg.push_str(e);
                                status_msg.push_str(fmt_segments[4]);
                            }
                        } else {
                            status_msg = format!("{}%", p_str);
                        }
                    } else {
                        status_msg = l.clone();
                    }

                    if let Ok(mut s) = state_clone.lock() {
                        *s = DownloadState::Downloading(p / 100.0, status_msg);
                    }
                }
            }

            if l.contains("Merging formats into \"") {
                if let Some(start) = l.find("Merging formats into \"") {
                    let raw_path = &l[start + "Merging formats into \"".len()..];
                    let clean_path = raw_path.trim().trim_end_matches('"');
                    *final_filename_clone.lock().unwrap() = Some(PathBuf::from(clean_path));
                }
            } else if l.contains("Destination: ") {
                if final_filename_clone.lock().unwrap().is_none()
                    && let Some(start) = l.find("Destination: ")
                {
                    let raw_path = &l[start + "Destination: ".len()..];
                    let clean_path = raw_path.trim();
                    if !clean_path.ends_with(".vtt")
                        && !clean_path.ends_with(".srt")
                        && !clean_path.ends_with(".ass")
                        && !clean_path.ends_with(".lrc")
                    {
                        *final_filename_clone.lock().unwrap() = Some(PathBuf::from(clean_path));
                    }
                }
            } else if l.contains(" has already been downloaded")
                && final_filename_clone.lock().unwrap().is_none()
                && let Some(end) = l.find(" has already been downloaded")
            {
                let start = if let Some(p) = l.find("[download] ") {
                    p + "[download] ".len()
                } else {
                    0
                };
                if start < end {
                    let filename = &l[start..end];
                    let clean_filename = filename.trim();
                    if !clean_filename.ends_with(".vtt")
                        && !clean_filename.ends_with(".srt")
                        && !clean_filename.ends_with(".ass")
                        && !clean_filename.ends_with(".lrc")
                    {
                        *final_filename_clone.lock().unwrap() = Some(PathBuf::from(clean_filename));
                    }
                }
            }
            if l.contains("[ExtractAudio] Destination: ")
                && let Some(start) = l.find("[ExtractAudio] Destination: ")
            {
                let raw_path = &l[start + "[ExtractAudio] Destination: ".len()..];
                let clean_path = raw_path.trim();
                *final_filename_clone.lock().unwrap() = Some(PathBuf::from(clean_path));
            }
            log(&logs_clone, l);
        }
    });

    let logs_clone_err = logs.clone();
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for l in reader.lines().map_while(Result::ok) {
            log(&logs_clone_err, format!("ERR: {}", l));
        }
    });

    let status = child.wait();
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    match status {
        Ok(exit_status) => {
            if exit_status.success() {
                Ok(final_filename.lock().unwrap().clone())
            } else {
                Err(format!("Exit Code: {}", exit_status))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn check_update_ytdlp_and_prepare_retry(
    bin_dir: &Path,
    state: &Arc<Mutex<DownloadState>>,
    logs: &Arc<Mutex<Vec<String>>>,
    ytdlp_status: &Arc<Mutex<InstallStatus>>,
    ytdlp_update_status: &Arc<Mutex<UpdateStatus>>,
    ytdlp_version: &Arc<Mutex<Option<String>>>,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<String, String> {
    set_download_stage(state, "Download failed. Checking yt-dlp update...");
    *ytdlp_update_status.lock().unwrap() = UpdateStatus::Checking;

    let ytdlp_path = bin_dir.join("yt-dlp.exe");
    let local_ver = read_local_ytdlp_version(&ytdlp_path).ok();
    if let Some(ver) = &local_ver {
        *ytdlp_version.lock().unwrap() = Some(ver.clone());
    }

    let remote_ver = match fetch_latest_ytdlp_version() {
        Ok(ver) => Some(ver),
        Err(e) => {
            log(
                logs,
                format!(
                    "Could not confirm latest yt-dlp version, forcing refresh: {}",
                    e
                ),
            );
            None
        }
    };

    if let (Some(local), Some(remote)) = (&local_ver, &remote_ver) {
        log(
            logs,
            format!("yt-dlp auto-fix check: local={}, remote={}", local, remote),
        );
        if local == remote {
            *ytdlp_status.lock().unwrap() = InstallStatus::Installed;
            *ytdlp_update_status.lock().unwrap() = UpdateStatus::UpToDate;
            return Ok(format!("yt-dlp is already up to date ({})", local));
        }
        *ytdlp_update_status.lock().unwrap() = UpdateStatus::UpdateAvailable(remote.clone());
    } else if let Some(remote) = &remote_ver {
        *ytdlp_update_status.lock().unwrap() = UpdateStatus::UpdateAvailable(remote.clone());
    }

    let stage_msg = if let Some(remote) = &remote_ver {
        format!("Updating yt-dlp to {}...", remote)
    } else {
        "Updating yt-dlp to latest...".to_string()
    };
    set_download_stage(state, stage_msg.clone());

    {
        let mut status = ytdlp_status.lock().unwrap();
        *status = InstallStatus::Downloading(0.0);
    }
    cancel_flag.store(false, Ordering::Relaxed);

    match download_file(YTDLP_DOWNLOAD_URL, &ytdlp_path, ytdlp_status, cancel_flag) {
        Ok(_) => {
            *ytdlp_status.lock().unwrap() = InstallStatus::Installed;

            let installed_ver = read_local_ytdlp_version(&ytdlp_path)
                .ok()
                .or(remote_ver.clone())
                .unwrap_or_else(|| "latest".to_string());
            *ytdlp_version.lock().unwrap() = Some(installed_ver.clone());

            if let Some(remote) = remote_ver {
                if installed_ver == remote {
                    *ytdlp_update_status.lock().unwrap() = UpdateStatus::UpToDate;
                } else {
                    *ytdlp_update_status.lock().unwrap() = UpdateStatus::UpdateAvailable(remote);
                }
            } else {
                *ytdlp_update_status.lock().unwrap() = UpdateStatus::Idle;
            }

            log(
                logs,
                format!("yt-dlp auto-refresh complete: {}", installed_ver),
            );
            Ok(format!("yt-dlp updated ({})", installed_ver))
        }
        Err(e) => {
            *ytdlp_status.lock().unwrap() = InstallStatus::Error(e.clone());
            *ytdlp_update_status.lock().unwrap() = UpdateStatus::Error(e.clone());
            log(logs, format!("yt-dlp auto-refresh failed: {}", e));
            Err(e)
        }
    }
}

impl DownloadManager {
    pub fn start_analysis(&mut self) {
        let idx = self.active_idx();
        let url = self.sessions[idx].input_url.trim().to_string();
        if url.is_empty() {
            return;
        }

        let bin_dir = self.bin_dir.clone();
        let cookie_browser = self.cookie_browser.clone();
        let formats_clone = self.sessions[idx].available_formats.clone();
        let manual_subs_clone = self.sessions[idx].available_subs_manual.clone();
        let use_subtitles_clone = self.use_subtitles.clone();
        let is_analyzing = self.sessions[idx].is_analyzing.clone();
        let error_clone = self.sessions[idx].analysis_error.clone();

        self.sessions[idx].last_url_analyzed = url.clone();
        *is_analyzing.lock().unwrap() = true;
        *error_clone.lock().unwrap() = None;

        // Reset analysis-specific choices for new URL
        formats_clone.lock().unwrap().clear();
        manual_subs_clone.lock().unwrap().clear();
        self.sessions[idx].selected_format = None;
        self.sessions[idx].selected_subtitle = None;

        use super::utils::fetch_video_formats;

        thread::spawn(
            move || match fetch_video_formats(&url, &bin_dir, cookie_browser) {
                Ok((formats, manual, _auto)) => {
                    *formats_clone.lock().unwrap() = formats;
                    *manual_subs_clone.lock().unwrap() = manual.clone();
                    if manual.is_empty() {
                        *use_subtitles_clone.lock().unwrap() = false;
                    }
                    *is_analyzing.lock().unwrap() = false;
                }
                Err(e) => {
                    *error_clone.lock().unwrap() = Some(e);
                    *is_analyzing.lock().unwrap() = false;
                }
            },
        );
    }

    pub fn start_media_download(&self, progress_fmt: String) {
        let idx = self.active_idx();
        let session = match self.sessions.get(idx) {
            Some(s) => s,
            None => return,
        };
        let url = session.input_url.trim().to_string();
        if url.is_empty() {
            return;
        }

        let bin_dir = self.bin_dir.clone();
        let download_type = session.download_type.clone();
        let state = session.download_state.clone();
        let logs = session.logs.clone();
        let ytdlp_status = self.ytdlp_status.clone();
        let ytdlp_update_status = self.ytdlp_update_status.clone();
        let ytdlp_version = self.ytdlp_version.clone();
        let cancel_flag = session.cancel_flag.clone();

        // Capture advanced flags
        let use_metadata = self.use_metadata;
        let use_sponsorblock = self.use_sponsorblock;
        let use_subtitles = *self.use_subtitles.lock().unwrap();
        let use_playlist = self.use_playlist;
        let cookie_browser = self.cookie_browser.clone();
        let selected_format = session.selected_format.clone();
        let selected_subtitle = session.selected_subtitle.clone();

        let download_path = self
            .custom_download_path
            .clone()
            .unwrap_or_else(|| dirs::download_dir().unwrap_or(PathBuf::from(".")));

        {
            let mut s = state.lock().unwrap();
            if matches!(*s, DownloadState::Downloading(_, _)) {
                return;
            }
            *s = DownloadState::Downloading(0.0, "Starting...".to_string());
        }

        thread::spawn(move || {
            log(&logs, format!("Processing URL: {}", url));
            let ytdlp_exe = bin_dir.join("yt-dlp.exe");

            let mut args = vec![
                "--encoding".to_string(),
                "utf-8".to_string(),
                "--ffmpeg-location".to_string(),
                bin_dir.to_string_lossy().to_string(),
            ];

            let deno_path = bin_dir.join("deno.exe");
            if deno_path.exists() {
                args.push("--js-runtimes".to_string());
                args.push(format!("deno:{}", deno_path.to_string_lossy()));
            }

            // Progress per line for potential parsing
            args.push("--newline".to_string());
            // Always re-download if quality differs (don't skip based on filename)
            args.push("--force-overwrites".to_string());

            if !use_playlist {
                args.push("--no-playlist".to_string());
            } else {
                args.push("--yes-playlist".to_string());
            }

            if use_metadata {
                args.push("--embed-metadata".to_string());
                args.push("--embed-chapters".to_string());
                args.push("--embed-thumbnail".to_string());
            }

            if use_sponsorblock {
                args.push("--sponsorblock-remove".to_string());
                args.push("all".to_string());
            }

            if use_subtitles {
                args.push("--write-subs".to_string());
                args.push("--sub-langs".to_string());
                if let Some(lang) = selected_subtitle {
                    args.push(lang);
                } else {
                    args.push("en.*,vi.*,ko.*".to_string());
                }
                args.push("--embed-subs".to_string());
            }

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

            match download_type {
                DownloadType::Video => {
                    args.push("-f".to_string());
                    if let Some(fmt_str) = selected_format {
                        // fmt_str is like "1080p"
                        let height = fmt_str.trim_end_matches('p');
                        // Use height<= for best available up to chosen quality
                        let selector =
                            format!("bestvideo[height<={0}]+bestaudio/best[height<={0}]", height);
                        args.push(selector);
                    } else {
                        args.push("bestvideo+bestaudio/best".to_string());
                    }
                    args.push("--merge-output-format".to_string());
                    args.push("mp4".to_string());
                }
                DownloadType::Audio => {
                    args.push("-x".to_string());
                    args.push("--audio-format".to_string());
                    args.push("mp3".to_string());
                    args.push("--audio-quality".to_string());
                    args.push("0".to_string());
                }
            }

            args.push("-o".to_string());
            let out_tmpl = download_path.join("%(title)s.%(ext)s");
            args.push(out_tmpl.to_string_lossy().to_string());

            args.push(url);

            match run_ytdlp_download_attempt(
                &ytdlp_exe,
                &args,
                &progress_fmt,
                &state,
                &logs,
                "initial",
            ) {
                Ok(final_path) => {
                    set_download_finished(&state, final_path);
                    log(&logs, "Download Finished Successfully.");
                }
                Err(first_err) => {
                    log(
                        &logs,
                        format!("Download failed on first attempt: {}", first_err),
                    );

                    match check_update_ytdlp_and_prepare_retry(
                        &bin_dir,
                        &state,
                        &logs,
                        &ytdlp_status,
                        &ytdlp_update_status,
                        &ytdlp_version,
                        &cancel_flag,
                    ) {
                        Ok(update_msg) => {
                            set_download_stage(
                                &state,
                                format!("{} - retrying download...", update_msg),
                            );
                            log(&logs, "Retrying download after yt-dlp refresh...");

                            match run_ytdlp_download_attempt(
                                &ytdlp_exe,
                                &args,
                                &progress_fmt,
                                &state,
                                &logs,
                                "retry",
                            ) {
                                Ok(final_path) => {
                                    set_download_finished(&state, final_path);
                                    log(&logs, "Download recovered after yt-dlp refresh.");
                                }
                                Err(retry_err) => {
                                    let combined_error = format!(
                                        "{} | Retry after yt-dlp refresh failed: {}",
                                        first_err, retry_err
                                    );
                                    *state.lock().unwrap() =
                                        DownloadState::Error(combined_error.clone());
                                    log(
                                        &logs,
                                        format!("Download failed after retry: {}", retry_err),
                                    );
                                }
                            }
                        }
                        Err(update_err) => {
                            let combined_error = format!(
                                "{} | yt-dlp auto-refresh failed: {}",
                                first_err, update_err
                            );
                            *state.lock().unwrap() = DownloadState::Error(combined_error.clone());
                            log(&logs, format!("Auto-refresh failed: {}", update_err));
                        }
                    }
                }
            }
        });
    }
}
