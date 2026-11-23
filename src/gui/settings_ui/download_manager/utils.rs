use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt as _;

use super::types::CookieBrowser;
use crate::component_registry::capabilities;
use crate::component_registry::external_tools::{self, ExternalTool};

pub(super) type VideoFormatLists = (Vec<String>, Vec<String>, Vec<String>);

/// The log view shows recent context only; a long playlist must not grow it
/// (or the per-frame text it renders) without bound.
const MAX_LOG_LINES: usize = 500;

pub fn log(logs: &Arc<Mutex<Vec<String>>>, msg: impl Into<String>) {
    let msg = msg.into();
    // Percent lines already drive the live status; keeping each one would
    // bury the lines that explain a failure.
    if is_download_progress_line(&msg) {
        return;
    }
    let mut logs = logs.lock().unwrap();
    if logs.len() >= MAX_LOG_LINES {
        let excess = logs.len() + 1 - MAX_LOG_LINES;
        logs.drain(..excess);
    }
    logs.push(msg);
}

fn is_download_progress_line(line: &str) -> bool {
    line.trim_start().starts_with("[download]") && line.contains('%')
}

pub fn fetch_video_formats(
    url: &str,
    cookie_browser: CookieBrowser,
    cancelled: &AtomicBool,
) -> Result<VideoFormatLists, String> {
    match fetch_video_formats_once(url, cookie_browser.clone(), cancelled) {
        Ok(formats) => Ok(formats),
        Err(error) if cancelled.load(Ordering::SeqCst) => Err(error),
        Err(first_error) => match external_tools::refresh_downloader_after_failure(true) {
            Ok(updated) if !updated.is_empty() && !cancelled.load(Ordering::SeqCst) => {
                fetch_video_formats_once(url, cookie_browser, cancelled)
            }
            Ok(_) => Err(first_error),
            Err(_) => Err(first_error),
        },
    }
}

fn fetch_video_formats_once(
    url: &str,
    cookie_browser: CookieBrowser,
    cancelled: &AtomicBool,
) -> Result<VideoFormatLists, String> {
    let ytdlp = capabilities::resolve_external_tool(ExternalTool::YtDlp, cancelled, |_| {})
        .map_err(|error| format!("Prepare pinned yt-dlp: {error:#}"))?;
    let deno = if cookie_browser == CookieBrowser::None {
        capabilities::acquire_external_tool(ExternalTool::Deno).ok()
    } else {
        Some(
            capabilities::resolve_external_tool(ExternalTool::Deno, cancelled, |_| {})
                .map_err(|error| format!("Prepare pinned Deno: {error:#}"))?,
        )
    };

    let deno_executable = deno.as_ref().map(|deno| deno.executable());
    let args = analysis_args(url, cookie_browser, deno_executable.as_deref());
    let mut cmd = Command::new(ytdlp.executable());
    cmd.args(&args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let (output, _) = super::analysis_process::run(&mut cmd, cancelled)?;
    parse_format_lists(&String::from_utf8_lossy(&output))
}

/// The `--dump-json` command line behind the Analyze step.
pub(super) fn analysis_args(
    url: &str,
    cookie_browser: CookieBrowser,
    deno_executable: Option<&std::path::Path>,
) -> Vec<String> {
    let mut args = Vec::new();
    append_isolation_args(&mut args);
    args.extend(["--dump-json", "--no-playlist"].map(str::to_string));
    if let Some(deno) = deno_executable {
        args.push("--js-runtimes".to_string());
        args.push(format!("deno:{}", deno.to_string_lossy()));
    }
    append_cookie_args(&mut args, cookie_browser);
    append_url_operand(&mut args, url);
    args
}

/// Resolutions (descending) and manual / automatic subtitle languages from
/// yt-dlp's `--dump-json` output.
pub(super) fn parse_format_lists(json_str: &str) -> Result<VideoFormatLists, String> {
    let v: serde_json::Value = serde_json::from_str(json_str).map_err(|e| e.to_string())?;

    // 1. Resolutions, labelled by the smaller dimension as yt-dlp's `res`
    //    sort key counts them: a 1080x1920 Short is "1080p", not "1920p".
    //    Only formats with a real video codec count.
    let mut heights = std::collections::HashSet::new();
    if let Some(formats) = v.get("formats").and_then(|f| f.as_array()) {
        for f in formats {
            // Skip audio-only and storyboard formats
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            if vcodec == "none" || vcodec == "images" {
                continue;
            }
            let height = f.get("height").and_then(|h| h.as_u64()).unwrap_or(0);
            let width = f.get("width").and_then(|w| w.as_u64()).unwrap_or(0);
            let resolution = match (width, height) {
                (0, h) => h,
                (w, 0) => w,
                (w, h) => w.min(h),
            };
            if resolution > 0 {
                heights.insert(resolution as u32);
            }
        }
    }

    let mut sorted_heights: Vec<u32> = heights.into_iter().collect();
    sorted_heights.sort_unstable_by(|a, b| b.cmp(a)); // Descending
    let format_results: Vec<String> = sorted_heights
        .into_iter()
        .map(|h| format!("{}p", h))
        .collect();

    // 2. Extract Subtitles
    let mut manual_langs = std::collections::HashSet::new();
    if let Some(subs) = v.get("subtitles").and_then(|s| s.as_object()) {
        for lang in subs.keys() {
            manual_langs.insert(lang.clone());
        }
    }

    let mut auto_langs = std::collections::HashSet::new();
    if let Some(auto_subs) = v.get("automatic_captions").and_then(|s| s.as_object()) {
        for lang in auto_subs.keys() {
            auto_langs.insert(lang.clone());
        }
    }

    let mut sorted_manual: Vec<String> = manual_langs.into_iter().collect();
    sorted_manual.sort();

    let mut sorted_auto: Vec<String> = auto_langs.into_iter().collect();
    sorted_auto.sort();

    Ok((format_results, sorted_manual, sorted_auto))
}

pub(super) fn append_isolation_args(args: &mut Vec<String>) {
    args.extend(["--ignore-config", "--no-plugin-dirs"].map(str::to_string));
}

/// Ends yt-dlp option parsing before the user's input, so a pasted video ID or
/// text that starts with `-` is fetched as a URL instead of read as an option.
pub(super) fn append_url_operand(args: &mut Vec<String>, url: &str) {
    args.push("--".to_string());
    args.push(url.to_string());
}

pub(super) fn bounded_tool_error(stderr: &str, fallback: &str) -> String {
    const MAX_LINES: usize = 12;
    const MAX_CHARS: usize = 4_096;
    let lines = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(MAX_LINES);
    let message = lines[start..].join("\n");
    if message.is_empty() {
        return fallback.to_string();
    }
    message.chars().take(MAX_CHARS).collect()
}

pub(super) fn append_cookie_args(args: &mut Vec<String>, cookie_browser: CookieBrowser) {
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
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_LOG_LINES, append_isolation_args, append_url_operand, bounded_tool_error, log,
        parse_format_lists,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn logs_keep_recent_explanatory_lines_and_skip_progress() {
        let logs = Arc::new(Mutex::new(Vec::new()));
        log(&logs, "[download]  12.5% of 3.00MiB at 1.00MiB/s ETA 00:02");
        for index in 0..(MAX_LOG_LINES + 20) {
            log(&logs, format!("line-{index}"));
        }
        log(&logs, "[download] Destination: clip.mp4");
        let logs = logs.lock().unwrap();
        assert_eq!(logs.len(), MAX_LOG_LINES);
        assert_eq!(
            logs.last().map(String::as_str),
            Some("[download] Destination: clip.mp4")
        );
        assert!(logs.iter().all(|line| !line.contains('%')));
        assert_eq!(logs[0], "line-21");
    }

    #[test]
    fn shared_recovery_fixture_requires_isolated_tool_execution() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/video-downloader/recovery.json"
        )))
        .unwrap();
        let mut args = Vec::new();
        append_isolation_args(&mut args);
        assert_eq!(args, ["--ignore-config", "--no-plugin-dirs"]);
        assert_eq!(fixture["commandIsolation"]["ignoreUserConfiguration"], true);
        assert_eq!(
            fixture["commandIsolation"]["disableUserPluginDirectories"],
            true
        );
        assert_eq!(fixture["failure"]["maximumRetries"], 1);
        assert_eq!(fixture["commandIsolation"]["endOptionsBeforeUrl"], true);
        let mut operand = Vec::new();
        append_url_operand(&mut operand, "-x");
        assert_eq!(operand, ["--", "-x"]);
    }

    #[test]
    fn dash_leading_input_stays_a_url_operand() {
        for input in ["-wNyEUrxzFU", "--exec=calc.exe"] {
            let mut args = vec!["--dump-json".to_string()];
            append_url_operand(&mut args, input);
            assert_eq!(args, ["--dump-json", "--", input]);
        }
    }

    #[test]
    fn tool_error_keeps_the_useful_bounded_tail() {
        let stderr = (0..20)
            .map(|index| format!("diagnostic {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = bounded_tool_error(&stderr, "fallback");
        assert!(!error.contains("diagnostic 7"));
        assert!(error.contains("diagnostic 8"));
        assert!(error.contains("diagnostic 19"));
        assert_eq!(bounded_tool_error(" \n", "fallback"), "fallback");
    }

    #[test]
    fn qualities_count_the_short_side_and_ignore_thumbnails() {
        let portrait = r#"{"formats": [
            {"vcodec": "avc1", "width": 1080, "height": 1920},
            {"vcodec": "vp9", "width": 720, "height": 1280},
            {"vcodec": "none", "height": 0},
            {"vcodec": "images", "width": 320, "height": 180}
        ]}"#;
        assert_eq!(parse_format_lists(portrait).unwrap().0, ["1080p", "720p"]);
        // An audio host lists no video format; its artwork sizes are not qualities.
        let audio = r#"{"formats": [{"vcodec": "none", "acodec": "mp3"}],
            "thumbnails": [{"url": "a", "width": 500, "height": 500}]}"#;
        assert!(parse_format_lists(audio).unwrap().0.is_empty());
    }
}
