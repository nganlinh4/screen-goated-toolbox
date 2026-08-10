use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::DownloadManager;
use super::types::{CookieBrowser, DownloadState, DownloadType, InstallStatus};
use super::utils::{append_cookie_args, fetch_video_formats, log};
use super::ytdlp_process::run_ytdlp_download_attempt;
use crate::component_registry::external_tools::{self, ExternalTool, ExternalToolUse};

fn set_download_stage(state: &Arc<Mutex<DownloadState>>, message: impl Into<String>) {
    *state.lock().unwrap() = DownloadState::Downloading(0.0, message.into());
}

fn set_download_finished(state: &Arc<Mutex<DownloadState>>, final_path: Option<PathBuf>) {
    *state.lock().unwrap() = DownloadState::Finished(
        final_path.unwrap_or_default(),
        "Download Completed!".to_string(),
    );
}

fn finish_if_cancelled(
    state: &Arc<Mutex<DownloadState>>,
    logs: &Arc<Mutex<Vec<String>>>,
    cancel: &Arc<AtomicBool>,
    error: &str,
) -> bool {
    if cancel.load(Ordering::Relaxed) || error == "Cancelled" {
        *state.lock().unwrap() = DownloadState::Idle;
        log(logs, "Download cancelled.");
        true
    } else {
        false
    }
}

fn prepare_tool(
    tool: ExternalTool,
    status: &Arc<Mutex<InstallStatus>>,
    state: &Arc<Mutex<DownloadState>>,
    cancel: &Arc<AtomicBool>,
) -> Result<ExternalToolUse, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    set_download_stage(state, format!("Preparing pinned {}...", tool.id()));
    *status.lock().unwrap() = InstallStatus::Downloading(0.0);
    let badge =
        crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&localized_tool_name(tool));
    let progress_status = status.clone();
    let result = external_tools::ensure(tool, cancel, move |done, total| {
        badge.report(done, total);
        *progress_status.lock().unwrap() =
            InstallStatus::Downloading(done as f32 / total.max(1) as f32);
    });
    match result {
        Ok(component) => {
            *status.lock().unwrap() = InstallStatus::Installed;
            Ok(component)
        }
        Err(error) if cancel.load(Ordering::Relaxed) => {
            *status.lock().unwrap() = InstallStatus::Missing;
            Err("Cancelled".to_string())
        }
        Err(error) => {
            let message = format!("Prepare {}: {error:#}", tool.id());
            *status.lock().unwrap() = InstallStatus::Error(message.clone());
            Err(message)
        }
    }
}

fn localized_tool_name(tool: ExternalTool) -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&language);
    match tool {
        ExternalTool::YtDlp => text.auxiliary.managed_tools.tool_ytdlp,
        ExternalTool::Ffmpeg => text.auxiliary.managed_tools.tool_ffmpeg,
        ExternalTool::Deno => text.auxiliary.managed_tools.tool_deno,
    }
    .to_string()
}

impl DownloadManager {
    pub fn start_analysis(&mut self) {
        let idx = self.active_idx();
        let url = self.sessions[idx].input_url.trim().to_string();
        if url.is_empty() {
            return;
        }

        let cookie_browser = self.cookie_browser.clone();
        let formats = self.sessions[idx].available_formats.clone();
        let manual_subtitles = self.sessions[idx].available_subs_manual.clone();
        let use_subtitles = self.use_subtitles.clone();
        let is_analyzing = self.sessions[idx].is_analyzing.clone();
        let error = self.sessions[idx].analysis_error.clone();

        self.sessions[idx].last_url_analyzed = url.clone();
        *is_analyzing.lock().unwrap() = true;
        *error.lock().unwrap() = None;
        formats.lock().unwrap().clear();
        manual_subtitles.lock().unwrap().clear();
        self.sessions[idx].selected_format = None;
        self.sessions[idx].selected_subtitle = None;

        std::thread::spawn(move || {
            match fetch_video_formats(&url, cookie_browser) {
                Ok((resolved_formats, manual, _automatic)) => {
                    *formats.lock().unwrap() = resolved_formats;
                    *manual_subtitles.lock().unwrap() = manual.clone();
                    if manual.is_empty() {
                        *use_subtitles.lock().unwrap() = false;
                    }
                }
                Err(message) => *error.lock().unwrap() = Some(message),
            }
            *is_analyzing.lock().unwrap() = false;
        });
    }

    pub fn start_media_download(&self, progress_format: String) {
        let idx = self.active_idx();
        let Some(session) = self.sessions.get(idx) else {
            return;
        };
        let url = session.input_url.trim().to_string();
        if url.is_empty() {
            return;
        }

        let download_type = session.download_type.clone();
        let state = session.download_state.clone();
        let logs = session.logs.clone();
        let cancel = session.cancel_flag.clone();
        let ytdlp_status = self.ytdlp_status.clone();
        let ffmpeg_status = self.ffmpeg_status.clone();
        let deno_status = self.deno_status.clone();
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
            let mut current = state.lock().unwrap();
            if matches!(*current, DownloadState::Downloading(_, _)) {
                return;
            }
            cancel.store(false, Ordering::Relaxed);
            *current = DownloadState::Downloading(0.0, "Starting...".to_string());
        }

        std::thread::spawn(move || {
            let attempt = |attempt_label: &str| -> Result<Option<PathBuf>, String> {
                log(&logs, format!("Processing URL: {url}"));
                let ytdlp = prepare_tool(ExternalTool::YtDlp, &ytdlp_status, &state, &cancel)?;
                let ffmpeg = prepare_tool(ExternalTool::Ffmpeg, &ffmpeg_status, &state, &cancel)?;
                let deno = if cookie_browser == CookieBrowser::None {
                    external_tools::acquire_installed(ExternalTool::Deno).ok()
                } else {
                    Some(prepare_tool(
                        ExternalTool::Deno,
                        &deno_status,
                        &state,
                        &cancel,
                    )?)
                };
                set_download_stage(&state, "Starting pinned yt-dlp...");

                let mut args = vec![
                    "--encoding".to_string(),
                    "utf-8".to_string(),
                    "--ffmpeg-location".to_string(),
                    ffmpeg.bin_dir().to_string_lossy().to_string(),
                    "--newline".to_string(),
                    "--force-overwrites".to_string(),
                ];
                if let Some(deno) = deno.as_ref() {
                    args.push("--js-runtimes".to_string());
                    args.push(format!("deno:{}", deno.executable().to_string_lossy()));
                }
                args.push(if use_playlist {
                    "--yes-playlist".to_string()
                } else {
                    "--no-playlist".to_string()
                });
                if use_metadata {
                    args.extend(
                        ["--embed-metadata", "--embed-chapters", "--embed-thumbnail"]
                            .map(str::to_string),
                    );
                }
                if use_sponsorblock {
                    args.extend(["--sponsorblock-remove", "all"].map(str::to_string));
                }
                if use_subtitles {
                    args.extend(["--write-subs", "--sub-langs"].map(str::to_string));
                    args.push(
                        selected_subtitle
                            .clone()
                            .unwrap_or_else(|| "en.*,vi.*,ko.*".to_string()),
                    );
                    args.push("--embed-subs".to_string());
                }
                append_cookie_args(&mut args, cookie_browser.clone());
                match &download_type {
                    DownloadType::Video => {
                        args.push("-f".to_string());
                        args.push(selected_format.clone().map_or_else(
                            || "bestvideo+bestaudio/best".to_string(),
                            |format| {
                                let height = format.trim_end_matches('p');
                                format!(
                                    "bestvideo[height<={height}]+bestaudio/best[height<={height}]"
                                )
                            },
                        ));
                        args.extend(["--merge-output-format", "mp4"].map(str::to_string));
                    }
                    DownloadType::Audio => {
                        args.extend(
                            ["-x", "--audio-format", "mp3", "--audio-quality", "0"]
                                .map(str::to_string),
                        );
                    }
                }
                args.push("-o".to_string());
                args.push(
                    download_path
                        .join("%(title)s.%(ext)s")
                        .to_string_lossy()
                        .to_string(),
                );
                args.push(url.clone());

                // Every selected tool guard remains alive until yt-dlp and its children exit.
                run_ytdlp_download_attempt(
                    &ytdlp.executable(),
                    &args,
                    &progress_format,
                    &state,
                    &logs,
                    &cancel,
                    attempt_label,
                )
            };

            let result = match attempt("verified") {
                Ok(path) => Ok(path),
                Err(error) if cancel.load(Ordering::Relaxed) => Err(error),
                Err(first_error) => {
                    log(
                        &logs,
                        "Download failed; checking the signed component catalog for newer downloader tools.",
                    );
                    match external_tools::refresh_downloader_after_failure(true) {
                        Ok(updated) if !updated.is_empty() => {
                            let names = updated
                                .iter()
                                .map(|tool| tool.id())
                                .collect::<Vec<_>>()
                                .join(", ");
                            log(
                                &logs,
                                format!(
                                    "Newer verified tools are available ({names}); retrying once."
                                ),
                            );
                            attempt("verified update")
                        }
                        Ok(_) => Err(first_error),
                        Err(refresh_error) => {
                            log(
                                &logs,
                                format!("Signed update check was unavailable: {refresh_error:#}"),
                            );
                            Err(first_error)
                        }
                    }
                }
            };

            match result {
                Ok(final_path) => {
                    set_download_finished(&state, final_path);
                    log(&logs, "Download Finished Successfully.");
                }
                Err(error) if finish_if_cancelled(&state, &logs, &cancel, &error) => {}
                Err(error) => {
                    *state.lock().unwrap() = DownloadState::Error(error.clone());
                    log(&logs, format!("Download failed: {error}"));
                }
            }
        });
    }
}
