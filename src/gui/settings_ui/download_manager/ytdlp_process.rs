use super::types::DownloadState;
use super::utils::log;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub(super) fn run_ytdlp_download_attempt(
    ytdlp_exe: &PathBuf,
    args: &[String],
    progress_fmt: &str,
    state: &Arc<Mutex<DownloadState>>,
    logs: &Arc<Mutex<Vec<String>>>,
    cancel_flag: &Arc<AtomicBool>,
    attempt_label: &str,
) -> Result<Option<PathBuf>, String> {
    use std::process::Stdio;
    use std::time::Duration;

    let mut cmd = std::process::Command::new(ytdlp_exe);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

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
    let stdout_cancel = cancel_flag.clone();
    let final_filename = Arc::new(Mutex::new(None));
    let final_filename_clone = final_filename.clone();
    let fmt_str = progress_fmt.to_string();

    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if stdout_cancel.load(Ordering::Relaxed) {
                break;
            }
            update_progress_from_line(&line, &fmt_str, &state_clone);
            capture_final_path_from_line(&line, &final_filename_clone);
            log(&logs_clone, line);
        }
    });

    let logs_clone_err = logs.clone();
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            log(&logs_clone_err, format!("ERR: {}", line));
        }
    });

    let status = loop {
        if cancel_flag.load(Ordering::Relaxed) {
            log(logs, "Cancelling yt-dlp download...");
            kill_process_tree(child.id(), logs);
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            cleanup_cancelled_download(&final_filename, logs);
            return Err("Cancelled".to_string());
        }

        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => break Err(e),
        }
    };
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    match status {
        Ok(exit_status) if exit_status.success() => Ok(final_filename.lock().unwrap().clone()),
        Ok(exit_status) => Err(format!("Exit Code: {}", exit_status)),
        Err(e) => Err(e.to_string()),
    }
}

fn kill_process_tree(pid: u32, logs: &Arc<Mutex<Vec<String>>>) {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
        cmd.creation_flags(0x08000000);
        let status = cmd.status();
        match status {
            Ok(status) if status.success() => {
                log(logs, format!("Killed yt-dlp process tree (pid={pid})."));
                return;
            }
            Ok(status) => {
                log(
                    logs,
                    format!("taskkill failed for yt-dlp process tree (pid={pid}): {status}"),
                );
            }
            Err(error) => {
                log(
                    logs,
                    format!("taskkill unavailable for yt-dlp process tree (pid={pid}): {error}"),
                );
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = pid;
}

fn cleanup_cancelled_download(
    final_filename: &Arc<Mutex<Option<PathBuf>>>,
    logs: &Arc<Mutex<Vec<String>>>,
) {
    let Some(path) = final_filename.lock().unwrap().clone() else {
        log(
            logs,
            "Cancel cleanup skipped: no destination path was detected yet.",
        );
        return;
    };

    let mut removed = 0usize;
    for candidate in cleanup_candidates(&path) {
        if remove_if_exists(&candidate, false) {
            removed += 1;
            log(
                logs,
                format!(
                    "Removed cancelled download artifact: {}",
                    candidate.display()
                ),
            );
        }
    }

    if remove_if_exists(&path, true) {
        removed += 1;
        log(
            logs,
            format!("Removed zero-byte cancelled output: {}", path.display()),
        );
    }

    if removed == 0 {
        log(
            logs,
            format!(
                "Cancel cleanup found no removable temp artifacts for: {}",
                path.display()
            ),
        );
    }
}

fn cleanup_candidates(path: &std::path::Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for suffix in [".part", ".ytdl", ".tmp", ".temp"] {
        candidates.push(append_file_name_suffix(path, suffix));
    }
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        candidates.push(path.with_extension(format!("{extension}.part")));
        candidates.push(path.with_extension(format!("{extension}.ytdl")));
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn append_file_name_suffix(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut candidate = path.to_path_buf();
    if let Some(file_name) = path.file_name() {
        let mut file_name = file_name.to_os_string();
        file_name.push(suffix);
        candidate.set_file_name(file_name);
    }
    candidate
}

fn remove_if_exists(path: &std::path::Path, zero_byte_only: bool) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() || (zero_byte_only && metadata.len() > 0) {
        return false;
    }
    std::fs::remove_file(path).is_ok()
}

fn update_progress_from_line(line: &str, progress_fmt: &str, state: &Arc<Mutex<DownloadState>>) {
    if !line.contains("[download]") || !line.contains("%") {
        return;
    }
    let Some(percent_end) = line.find("%") else {
        return;
    };
    let substr = &line[..percent_end];
    let Some(space) = substr.rfind(' ') else {
        return;
    };
    let Ok(progress) = substr[space + 1..].parse::<f32>() else {
        return;
    };

    let status_msg = build_status_message(line, progress_fmt);
    if let Ok(mut state) = state.lock() {
        *state = DownloadState::Downloading(progress / 100.0, status_msg);
    }
}

fn build_status_message(line: &str, progress_fmt: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut percent = None;
    let mut total = None;
    let mut speed = None;
    let mut eta = None;

    for (i, part) in parts.iter().enumerate() {
        if part.contains("%") {
            percent = Some(part.trim_end_matches('%'));
        } else if *part == "of" && i + 1 < parts.len() {
            total = known_value(parts[i + 1]);
        } else if *part == "at" && i + 1 < parts.len() {
            speed = known_value(parts[i + 1]);
        } else if *part == "ETA" && i + 1 < parts.len() {
            eta = known_value(parts[i + 1]);
        }
    }

    let Some(percent) = percent else {
        return line.to_string();
    };
    let segments: Vec<&str> = progress_fmt.split("{}").collect();
    if segments.len() < 5 {
        return format!("{}%", percent);
    }

    let mut status = String::new();
    status.push_str(segments[0]);
    status.push_str(percent);
    if let Some(total) = total {
        status.push_str(segments[1]);
        status.push_str(total);
    } else {
        status.push('%');
    }
    if let Some(speed) = speed {
        status.push_str(segments[2]);
        status.push_str(speed);
    }
    if let Some(eta) = eta {
        status.push_str(segments[3]);
        status.push_str(eta);
        status.push_str(segments[4]);
    }
    status
}

fn known_value(value: &str) -> Option<&str> {
    (value != "Unknown" && value != "N/A").then_some(value)
}

fn capture_final_path_from_line(line: &str, final_filename: &Arc<Mutex<Option<PathBuf>>>) {
    if let Some(path) = path_after(line, "Merging formats into \"") {
        *final_filename.lock().unwrap() = Some(PathBuf::from(path.trim_end_matches('"')));
    } else if final_filename.lock().unwrap().is_none() {
        if let Some(path) = path_after(line, "Destination: ") {
            if !is_subtitle_path(path) {
                *final_filename.lock().unwrap() = Some(PathBuf::from(path));
            }
        } else if let Some(path) = already_downloaded_path(line) {
            if !is_subtitle_path(path) {
                *final_filename.lock().unwrap() = Some(PathBuf::from(path));
            }
        }
    }

    if let Some(path) = path_after(line, "[ExtractAudio] Destination: ") {
        *final_filename.lock().unwrap() = Some(PathBuf::from(path));
    }
}

fn path_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker)
        .map(|start| line[start + marker.len()..].trim())
}

fn already_downloaded_path(line: &str) -> Option<&str> {
    let end = line.find(" has already been downloaded")?;
    let start = line
        .find("[download] ")
        .map(|pos| pos + "[download] ".len())
        .unwrap_or(0);
    (start < end).then(|| line[start..end].trim())
}

fn is_subtitle_path(path: &str) -> bool {
    [".vtt", ".srt", ".ass", ".lrc"]
        .iter()
        .any(|ext| path.ends_with(ext))
}
