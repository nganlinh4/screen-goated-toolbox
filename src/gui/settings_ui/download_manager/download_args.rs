//! The yt-dlp command line for a media download, built in one place so the app
//! and its tests run the exact same arguments.

use std::path::Path;

use super::types::{CookieBrowser, DownloadType};
use super::utils::{append_cookie_args, append_isolation_args, append_url_operand};

/// Output name: the title, capped at 150 UTF-8 bytes, then the extension.
///
/// Sites such as Facebook, X and TikTok use the whole post text as the title,
/// which can run past Windows' 255-character limit for one file name; yt-dlp
/// then fails with `[Errno 22] Invalid argument`. The cap leaves room for the
/// extension and yt-dlp's intermediate suffixes (`.f137`, `.temp`, `.part`).
/// The `B` flag counts bytes and never splits a character.
pub(super) const OUTPUT_TEMPLATE: &str = "%(title).150B.%(ext)s";

pub(super) struct DownloadRequest<'a> {
    pub url: &'a str,
    pub download_type: &'a DownloadType,
    pub download_path: &'a Path,
    pub ffmpeg_dir: &'a Path,
    pub deno_executable: Option<&'a Path>,
    pub cookie_browser: CookieBrowser,
    /// A resolution label such as `"720p"`; `None` takes the best available.
    pub selected_format: Option<&'a str>,
    pub selected_subtitle: Option<&'a str>,
    pub use_playlist: bool,
    pub use_metadata: bool,
    pub use_sponsorblock: bool,
    pub use_subtitles: bool,
}

/// The number in a quality label such as `"1080p"`.
///
/// The label is passed to yt-dlp as a *preference* (`-S res:N`), never a
/// filter. A filter such as `[height<=1080]` fails outright on sites whose
/// formats carry no height (direct links, audio hosts) and caps portrait video
/// by its long side, so a 1080x1920 Short saved as "1080p" came out 608x1080.
/// `res` is the smaller dimension: the best format at or below `N`, else the
/// smallest one above it.
fn resolution_limit(label: &str) -> Option<u32> {
    label
        .trim()
        .trim_end_matches('p')
        .parse()
        .ok()
        .filter(|&n| n > 0)
}

pub(super) fn build_download_args(request: &DownloadRequest<'_>) -> Vec<String> {
    let mut args = Vec::new();
    append_isolation_args(&mut args);
    args.extend([
        "--encoding".to_string(),
        "utf-8".to_string(),
        "--ffmpeg-location".to_string(),
        request.ffmpeg_dir.to_string_lossy().to_string(),
        "--newline".to_string(),
        "--force-overwrites".to_string(),
    ]);
    if let Some(deno) = request.deno_executable {
        args.push("--js-runtimes".to_string());
        args.push(format!("deno:{}", deno.to_string_lossy()));
    }
    args.push(if request.use_playlist {
        "--yes-playlist".to_string()
    } else {
        "--no-playlist".to_string()
    });
    if request.use_metadata {
        args.extend(
            ["--embed-metadata", "--embed-chapters", "--embed-thumbnail"].map(str::to_string),
        );
    }
    if request.use_sponsorblock {
        args.extend(["--sponsorblock-remove", "all"].map(str::to_string));
    }
    if request.use_subtitles {
        args.extend(["--write-subs", "--sub-langs"].map(str::to_string));
        args.push(
            request
                .selected_subtitle
                .unwrap_or("en.*,vi.*,ko.*")
                .to_string(),
        );
        args.push("--embed-subs".to_string());
    }
    append_cookie_args(&mut args, request.cookie_browser.clone());
    match request.download_type {
        DownloadType::Video => {
            args.extend(["-f", "bv*+ba/b"].map(str::to_string));
            if let Some(resolution) = request.selected_format.and_then(resolution_limit) {
                args.push("-S".to_string());
                args.push(format!("res:{resolution}"));
            }
            args.extend(["--merge-output-format", "mp4"].map(str::to_string));
            args.extend(
                [
                    "--print",
                    super::quality::COMPLETED_QUALITY_TEMPLATE,
                    "--no-quiet",
                    "--no-simulate",
                ]
                .map(str::to_owned),
            );
        }
        DownloadType::Audio => {
            args.extend(
                ["-x", "--audio-format", "mp3", "--audio-quality", "0"].map(str::to_string),
            );
        }
    }
    args.push("-o".to_string());
    args.push(
        request
            .download_path
            .join(OUTPUT_TEMPLATE)
            .to_string_lossy()
            .to_string(),
    );
    append_url_operand(&mut args, request.url);
    args
}

#[cfg(test)]
#[path = "download_args_tests.rs"]
mod tests;
