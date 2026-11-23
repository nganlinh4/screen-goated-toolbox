use super::quality::{COMPLETED_QUALITY_PREFIX, common_quality, reported_quality};
use super::types::DownloadState;
use super::utils::log;
use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::process_job::ContainedChild;

const STDERR_TAIL_LINES: usize = 12;
const STDERR_LINE_CHARS: usize = 2_048;

/// The job of the most recent launch, so tests can inspect containment.
#[cfg(test)]
static LAST_LAUNCH: Mutex<Option<(u32, crate::process_job::JobControl)>> = Mutex::new(None);

#[derive(Debug)]
pub(super) struct CompletedDownload {
    pub path: Option<PathBuf>,
    pub quality: Option<u32>,
}

pub(super) fn run_ytdlp_download_attempt(
    ytdlp_exe: &PathBuf,
    args: &[String],
    progress_fmt: &str,
    state: &Arc<Mutex<DownloadState>>,
    logs: &Arc<Mutex<Vec<String>>>,
    cancel_flag: &Arc<AtomicBool>,
    attempt_label: &str,
) -> Result<CompletedDownload, String> {
    use std::process::Stdio;
    use std::time::Duration;

    let mut cmd = std::process::Command::new(ytdlp_exe);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // The kill-on-close job ties yt-dlp and its ffmpeg children to this
    // attempt: cancelling, closing the tab or exiting the app ends the tree.
    let mut child = ContainedChild::spawn(&mut cmd).map_err(|e| e.to_string())?;
    #[cfg(test)]
    {
        *LAST_LAUNCH.lock().unwrap() = Some((child.id(), child.control()));
    }
    log(
        logs,
        format!("Running yt-dlp ({attempt_label}, pid {})...", child.id()),
    );
    let (Some(stdout), Some(stderr)) = (child.take_stdout(), child.take_stderr()) else {
        stop_contained(&mut child);
        return Err("Failed to open yt-dlp output".to_string());
    };

    let logs_clone = logs.clone();
    let state_clone = state.clone();
    let stdout_cancel = cancel_flag.clone();
    let final_filename = Arc::new(Mutex::new(None));
    let final_filename_clone = final_filename.clone();
    let fmt_str = progress_fmt.to_string();

    let stdout_thread = thread::spawn(move || {
        let mut qualities = HashSet::new();
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if stdout_cancel.load(Ordering::Relaxed) {
                break;
            }
            if let Some(record) = line.strip_prefix(COMPLETED_QUALITY_PREFIX) {
                qualities.insert(reported_quality(record));
                continue;
            }
            update_progress_from_line(&line, &fmt_str, &state_clone);
            capture_final_path_from_line(&line, &final_filename_clone);
            log(&logs_clone, line);
        }
        common_quality(&qualities)
    });

    let logs_clone_err = logs.clone();
    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
    let stderr_tail_writer = stderr_tail.clone();
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            log(&logs_clone_err, format!("ERR: {}", line));
            push_stderr_tail(&stderr_tail_writer, &line);
        }
    });

    if let Err(error) = child.resume() {
        stop_contained(&mut child);
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        return Err(error.to_string());
    }

    let status = loop {
        if cancel_flag.load(Ordering::Relaxed) {
            log(logs, "Cancelling yt-dlp download...");
            stop_contained(&mut child);
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
    let quality = stdout_thread.join().unwrap_or_default();
    let _ = stderr_thread.join();

    match status {
        Ok(exit_status) if exit_status.success() => Ok(CompletedDownload {
            path: final_filename.lock().unwrap().clone(),
            quality,
        }),
        Ok(exit_status) => Err(exit_failure_message(exit_status, &stderr_tail)),
        Err(e) => Err(e.to_string()),
    }
}

fn push_stderr_tail(tail: &Mutex<VecDeque<String>>, line: &str) {
    let mut tail = tail.lock().unwrap();
    if tail.len() == STDERR_TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line.chars().take(STDERR_LINE_CHARS).collect());
}

fn exit_failure_message(exit_status: ExitStatus, tail: &Mutex<VecDeque<String>>) -> String {
    let tail = tail.lock().unwrap();
    if tail.is_empty() {
        return format!("Exit Code: {exit_status}");
    }
    format!(
        "Exit Code: {exit_status}\nyt-dlp stderr:\n{}",
        tail.iter().cloned().collect::<Vec<_>>().join("\n")
    )
}

/// Ends the whole job, then waits until every member has exited so the output
/// pipes close and no descendant keeps writing into the destination.
fn stop_contained(child: &mut ContainedChild) {
    let _ = child.terminate();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !matches!(child.active_processes(), Ok(0)) && std::time::Instant::now() < deadline {
        thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.try_wait();
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
        } else if let Some(path) = already_downloaded_path(line)
            && !is_subtitle_path(path)
        {
            *final_filename.lock().unwrap() = Some(PathBuf::from(path));
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

#[cfg(test)]
mod tests {
    use super::{
        DownloadState, STDERR_LINE_CHARS, STDERR_TAIL_LINES, capture_final_path_from_line,
        push_stderr_tail, update_progress_from_line,
    };
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    const PROGRESS_FMT: &str = "{}% of {} at {}, ETA {}";

    fn progress(line: &str) -> DownloadState {
        let state = Arc::new(Mutex::new(DownloadState::Idle));
        update_progress_from_line(line, PROGRESS_FMT, &state);
        state.lock().unwrap().clone()
    }

    #[test]
    fn progress_lines_report_fraction_and_known_fields_only() {
        assert_eq!(
            progress("[download]  42.5% of 10.00MiB at 1.00MiB/s ETA 00:05"),
            DownloadState::Downloading(0.425, "42.5% of 10.00MiB at 1.00MiB/s, ETA 00:05".into())
        );
        assert_eq!(
            progress("[download]  10.0% of Unknown at N/A ETA Unknown"),
            DownloadState::Downloading(0.1, "10.0%".into())
        );
        assert_eq!(progress("[Merger] 100% merged"), DownloadState::Idle);
    }

    #[test]
    fn final_path_prefers_the_merged_file_and_ignores_subtitles() {
        let final_path = Arc::new(Mutex::new(None));
        for line in [
            "[info] Writing video subtitles to: clip.en.vtt",
            "[download] Destination: clip.en.vtt",
            "[download] Destination: clip.f137.mp4",
            "[Merger] Merging formats into \"clip.mp4\"",
            "[download] Destination: clip.f140.m4a",
        ] {
            capture_final_path_from_line(line, &final_path);
        }
        assert_eq!(*final_path.lock().unwrap(), Some(PathBuf::from("clip.mp4")));

        let already = Arc::new(Mutex::new(None));
        capture_final_path_from_line("[download] song.mp3 has already been downloaded", &already);
        assert_eq!(*already.lock().unwrap(), Some(PathBuf::from("song.mp3")));
    }

    #[test]
    fn stderr_tail_is_bounded_and_keeps_the_latest_lines() {
        let tail = Mutex::new(VecDeque::new());
        for index in 0..=STDERR_TAIL_LINES {
            push_stderr_tail(&tail, &format!("line-{index}"));
        }

        let tail = tail.lock().unwrap();
        assert_eq!(tail.len(), STDERR_TAIL_LINES);
        assert_eq!(tail.front().map(String::as_str), Some("line-1"));
        let expected_last = format!("line-{STDERR_TAIL_LINES}");
        assert_eq!(
            tail.back().map(String::as_str),
            Some(expected_last.as_str())
        );
    }

    #[test]
    fn stderr_tail_caps_individual_line_length() {
        let tail = Mutex::new(VecDeque::new());
        push_stderr_tail(&tail, &"x".repeat(STDERR_LINE_CHARS + 1));
        assert_eq!(
            tail.lock().unwrap().front().map(String::len),
            Some(STDERR_LINE_CHARS)
        );
    }

    #[test]
    fn a_running_download_is_contained_and_cancellation_ends_it() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows::Win32::System::JobObjects::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            WaitForSingleObject,
        };
        let ping = crate::system_command::system_executable("PING.EXE").unwrap();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let runner = {
            let (logs, cancel) = (logs.clone(), cancel.clone());
            std::thread::spawn(move || {
                let args = ["-n", "30", "127.0.0.1"].map(String::from);
                let state = Arc::new(Mutex::new(DownloadState::Idle));
                super::run_ytdlp_download_attempt(
                    &ping,
                    &args,
                    "",
                    &state,
                    &logs,
                    &cancel,
                    "contained",
                )
            })
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (pid, job) = loop {
            if let Some(launch) = super::LAST_LAUNCH.lock().unwrap().take() {
                break launch;
            }
            assert!(std::time::Instant::now() < deadline, "nothing was launched");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .unwrap();
        let contained = job.contains_process(process).unwrap();
        let kill_on_close = job
            .limits()
            .unwrap()
            .BasicLimitInformation
            .LimitFlags
            .contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
        cancel.store(true, Ordering::SeqCst);
        let result = runner.join().unwrap();
        let exited = unsafe { WaitForSingleObject(process, 5_000) } == WAIT_OBJECT_0;
        unsafe { CloseHandle(process) }.unwrap();
        assert!(contained, "the download process ran outside its job");
        assert!(kill_on_close, "the download job outlives its owner");
        assert!(matches!(result, Err(ref error) if error == "Cancelled"));
        assert!(exited);
    }
}
