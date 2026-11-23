//! Live edge-case runs through the app's own Analyze and download command
//! lines. Ignored by default: they hit the network and need local tools.
//!
//! `SGT_DL_CASES` names a JSON array of cases, `SGT_DL_YTDLP` the yt-dlp
//! executable, `SGT_DL_FFMPEG_DIR` the ffmpeg bin directory, `SGT_DL_OUT` the
//! download root and `SGT_DL_RESULTS` the JSON-lines report. `SGT_DL_DENO` is
//! optional, as it is in the app when no cookie browser is chosen.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::download_args::{DownloadRequest, build_download_args};
use super::types::{CookieBrowser, DownloadState, DownloadType};
use super::utils::{analysis_args, parse_format_lists};
use super::ytdlp_process::run_ytdlp_download_attempt;

#[derive(Deserialize)]
#[serde(default)]
struct Case {
    name: String,
    url: String,
    audio: bool,
    /// A height such as "360p", as the format picker stores it.
    format: Option<String>,
    subtitles: bool,
    subtitle: Option<String>,
    metadata: bool,
    sponsorblock: bool,
    playlist: bool,
    /// Extra folder under the download root, to test unusual paths.
    subdir: String,
    analyze: bool,
    timeout_secs: u64,
}

impl Default for Case {
    fn default() -> Self {
        Self {
            name: String::new(),
            url: String::new(),
            audio: false,
            format: None,
            subtitles: false,
            subtitle: None,
            metadata: true,
            sponsorblock: false,
            playlist: false,
            subdir: String::new(),
            analyze: true,
            timeout_secs: 300,
        }
    }
}

struct Tools {
    ytdlp: PathBuf,
    ffmpeg_dir: PathBuf,
    deno: Option<PathBuf>,
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set")))
}

fn analyze(tools: &Tools, case: &Case) -> serde_json::Value {
    let args = analysis_args(&case.url, CookieBrowser::None, tools.deno.as_deref());
    let mut cmd = Command::new(&tools.ytdlp);
    cmd.args(&args);
    let started = Instant::now();
    let result = super::analysis_process::run(&mut cmd, &AtomicBool::new(false))
        .and_then(|(out, _)| parse_format_lists(&String::from_utf8_lossy(&out)));
    let secs = started.elapsed().as_secs_f32();
    match result {
        Ok((heights, manual, auto)) => serde_json::json!({
            "ok": true, "secs": secs, "heights": heights,
            "manual_subs": manual.len(), "auto_subs": auto.len(),
        }),
        Err(error) => serde_json::json!({ "ok": false, "secs": secs, "error": error }),
    }
}

fn download(tools: &Tools, case: &Case, root: &Path) -> serde_json::Value {
    let download_path = root.join(&case.subdir);
    std::fs::create_dir_all(&download_path).unwrap();
    let download_type = if case.audio {
        DownloadType::Audio
    } else {
        DownloadType::Video
    };
    let args = build_download_args(&DownloadRequest {
        url: &case.url,
        download_type: &download_type,
        download_path: &download_path,
        ffmpeg_dir: &tools.ffmpeg_dir,
        deno_executable: tools.deno.as_deref(),
        cookie_browser: CookieBrowser::None,
        selected_format: case.format.as_deref(),
        selected_subtitle: case.subtitle.as_deref(),
        use_playlist: case.playlist,
        use_metadata: case.metadata,
        use_sponsorblock: case.sponsorblock,
        use_subtitles: case.subtitles,
    });
    let state = Arc::new(Mutex::new(DownloadState::Idle));
    let logs = Arc::new(Mutex::new(Vec::new()));
    let cancel = Arc::new(AtomicBool::new(false));
    let watchdog_cancel = cancel.clone();
    let deadline = Duration::from_secs(case.timeout_secs);
    let finished = Arc::new(AtomicBool::new(false));
    let watchdog_finished = finished.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        while !watchdog_finished.load(Ordering::Relaxed) {
            if started.elapsed() > deadline {
                watchdog_cancel.store(true, Ordering::Relaxed);
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
    let started = Instant::now();
    let result = run_ytdlp_download_attempt(
        &tools.ytdlp,
        &args,
        "{} {}",
        &state,
        &logs,
        &cancel,
        "live-test",
    );
    finished.store(true, Ordering::Relaxed);
    let secs = started.elapsed().as_secs_f32();
    let files: Vec<String> = std::fs::read_dir(&download_path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_file())
                .map(|entry| {
                    format!(
                        "{} ({} B)",
                        entry.file_name().to_string_lossy(),
                        entry.metadata().map(|m| m.len()).unwrap_or(0)
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let log_tail: Vec<String> = logs
        .lock()
        .unwrap()
        .iter()
        .rev()
        .take(15)
        .rev()
        .cloned()
        .collect();
    match result {
        Ok(result) => {
            let path = result.path;
            let exists = path.as_ref().is_some_and(|path| path.is_file());
            serde_json::json!({
                "ok": true, "secs": secs, "final_path": path, "final_exists": exists,
                "reported_quality": result.quality,
                "files": files, "log_tail": if exists { Vec::new() } else { log_tail },
            })
        }
        Err(error) => serde_json::json!({
            "ok": false, "secs": secs, "timed_out": cancel.load(Ordering::Relaxed),
            "error": error, "files": files, "log_tail": log_tail,
        }),
    }
}

#[test]
#[ignore = "downloads from the network; run with the SGT_DL_* variables set"]
fn live_download_cases() {
    let tools = Tools {
        ytdlp: env_path("SGT_DL_YTDLP"),
        ffmpeg_dir: env_path("SGT_DL_FFMPEG_DIR"),
        deno: std::env::var("SGT_DL_DENO").ok().map(PathBuf::from),
    };
    let cases: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(env_path("SGT_DL_CASES")).unwrap()).unwrap();
    let root = env_path("SGT_DL_OUT");
    let report = env_path("SGT_DL_RESULTS");
    let only = std::env::var("SGT_DL_ONLY").ok();
    let mut lines = String::new();
    for case in cases.iter().filter(|case| {
        only.as_ref()
            .is_none_or(|only| only.split(',').any(|n| n == case.name))
    }) {
        let case_root = root.join(&case.name);
        let analysis = case.analyze.then(|| analyze(&tools, case));
        let download = download(&tools, case, &case_root);
        let line = serde_json::json!({ "name": case.name, "url": case.url, "analysis": analysis, "download": download });
        println!("{line}");
        lines.push_str(&line.to_string());
        lines.push('\n');
        std::fs::write(&report, &lines).unwrap();
    }
}
