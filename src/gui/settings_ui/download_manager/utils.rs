use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt as _;

use super::types::CookieBrowser;
use crate::component_registry::capabilities;
use crate::component_registry::external_tools::{self, ExternalTool};

type VideoFormatLists = (Vec<String>, Vec<String>, Vec<String>);

pub fn log(logs: &Arc<Mutex<Vec<String>>>, msg: impl Into<String>) {
    logs.lock().unwrap().push(msg.into());
}

pub fn fetch_video_formats(
    url: &str,
    cookie_browser: CookieBrowser,
) -> Result<VideoFormatLists, String> {
    match fetch_video_formats_once(url, cookie_browser.clone()) {
        Ok(formats) => Ok(formats),
        Err(first_error) => match external_tools::refresh_downloader_after_failure(true) {
            Ok(updated) if !updated.is_empty() => fetch_video_formats_once(url, cookie_browser),
            Ok(_) => Err(first_error),
            Err(_) => Err(first_error),
        },
    }
}

fn fetch_video_formats_once(
    url: &str,
    cookie_browser: CookieBrowser,
) -> Result<VideoFormatLists, String> {
    let cancelled = AtomicBool::new(false);
    let ytdlp = capabilities::resolve_external_tool(ExternalTool::YtDlp, &cancelled, |_| {})
        .map_err(|error| format!("Prepare pinned yt-dlp: {error:#}"))?;
    let deno = if cookie_browser == CookieBrowser::None {
        capabilities::acquire_external_tool(ExternalTool::Deno).ok()
    } else {
        Some(
            capabilities::resolve_external_tool(ExternalTool::Deno, &cancelled, |_| {})
                .map_err(|error| format!("Prepare pinned Deno: {error:#}"))?,
        )
    };

    let mut args = Vec::new();
    append_isolation_args(&mut args);
    args.extend(["--dump-json", "--no-playlist"].map(str::to_string));

    if let Some(deno) = deno.as_ref() {
        args.push("--js-runtimes".to_string());
        args.push(format!("deno:{}", deno.executable().to_string_lossy()));
    }

    append_cookie_args(&mut args, cookie_browser);

    args.push(url.to_string());

    let mut cmd = Command::new(ytdlp.executable());
    cmd.args(&args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output().map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(bounded_tool_error(
            &String::from_utf8_lossy(&output.stderr),
            "yt-dlp could not analyze this URL",
        ));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

    // 1. Extract resolutions — only from formats that have a real video codec
    let mut heights = std::collections::HashSet::new();
    if let Some(formats) = v.get("formats").and_then(|f| f.as_array()) {
        for f in formats {
            // Skip audio-only and storyboard formats
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            if vcodec == "none" || vcodec == "images" {
                continue;
            }
            if let Some(h) = f.get("height").and_then(|h| h.as_u64())
                && h > 0
            {
                heights.insert(h as u32);
            }
        }
    }

    // Fallback Robust manual parsing for "height": 123 if JSON array failed for some reason
    if heights.is_empty() {
        let key = "\"height\":";
        for (i, _) in json_str.match_indices(key) {
            let after_key = &json_str[i + key.len()..];
            let num_start_idx = after_key.find(|c: char| !c.is_whitespace()).unwrap_or(0);
            let after_ws = &after_key[num_start_idx..];
            let num_end_idx = after_ws
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_ws.len());
            if num_end_idx > 0 {
                let num_str = &after_ws[..num_end_idx];
                if let Ok(h) = num_str.parse::<u32>()
                    && h > 0
                {
                    heights.insert(h);
                }
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
    use super::{append_isolation_args, bounded_tool_error};

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
}
