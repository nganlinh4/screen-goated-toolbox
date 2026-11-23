use super::*;
use std::path::PathBuf;

fn request<'a>(
    download_type: &'a DownloadType,
    download_path: &'a Path,
    ffmpeg_dir: &'a Path,
) -> DownloadRequest<'a> {
    DownloadRequest {
        url: "-https://example.com/watch?v=1",
        download_type,
        download_path,
        ffmpeg_dir,
        deno_executable: None,
        cookie_browser: CookieBrowser::None,
        selected_format: None,
        selected_subtitle: None,
        use_playlist: false,
        use_metadata: false,
        use_sponsorblock: false,
        use_subtitles: false,
    }
}

fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
    let at = args.iter().position(|arg| arg == flag).unwrap();
    &args[at + 1]
}

#[test]
fn output_name_caps_the_title_in_bytes() {
    let dir = PathBuf::from(r"C:\Downloads");
    let ffmpeg = PathBuf::from(r"C:\ffmpeg");
    let args = build_download_args(&request(&DownloadType::Video, &dir, &ffmpeg));
    assert_eq!(
        value_after(&args, "-o"),
        r"C:\Downloads\%(title).150B.%(ext)s"
    );
}

#[test]
fn the_url_is_the_last_operand_after_the_option_terminator() {
    let dir = PathBuf::from("out");
    let ffmpeg = PathBuf::from("ff");
    let args = build_download_args(&request(&DownloadType::Video, &dir, &ffmpeg));
    assert_eq!(
        &args[args.len() - 2..],
        ["--", "-https://example.com/watch?v=1"]
    );
    assert_eq!(&args[..2], ["--ignore-config", "--no-plugin-dirs"]);
}

#[test]
fn a_selected_resolution_is_a_preference_not_a_filter() {
    let dir = PathBuf::from("out");
    let ffmpeg = PathBuf::from("ff");
    let mut req = request(&DownloadType::Video, &dir, &ffmpeg);
    let best = build_download_args(&req);
    assert_eq!(value_after(&best, "-f"), "bv*+ba/b");
    assert!(!best.contains(&"-S".to_string()));
    req.selected_format = Some("720p");
    let args = build_download_args(&req);
    assert_eq!(value_after(&args, "-f"), "bv*+ba/b");
    assert_eq!(value_after(&args, "-S"), "res:720");
    assert_eq!(value_after(&args, "--merge-output-format"), "mp4");
    // A label that is not a resolution takes the best available.
    req.selected_format = Some("audio only");
    assert!(!build_download_args(&req).contains(&"-S".to_string()));
}

#[test]
fn audio_extracts_mp3_without_a_video_format() {
    let dir = PathBuf::from("out");
    let ffmpeg = PathBuf::from("ff");
    let args = build_download_args(&request(&DownloadType::Audio, &dir, &ffmpeg));
    assert_eq!(value_after(&args, "--audio-format"), "mp3");
    assert!(args.contains(&"-x".to_string()));
    assert!(!args.contains(&"-f".to_string()));
}

#[test]
fn options_carry_deno_cookies_subtitles_and_playlist_choice() {
    let dir = PathBuf::from("out");
    let ffmpeg = PathBuf::from("ff");
    let deno = PathBuf::from(r"C:\deno\deno.exe");
    let mut req = request(&DownloadType::Video, &dir, &ffmpeg);
    req.deno_executable = Some(&deno);
    req.cookie_browser = CookieBrowser::Firefox;
    req.use_subtitles = true;
    req.use_playlist = true;
    let args = build_download_args(&req);
    assert_eq!(
        value_after(&args, "--js-runtimes"),
        r"deno:C:\deno\deno.exe"
    );
    assert_eq!(value_after(&args, "--cookies-from-browser"), "firefox");
    assert_eq!(value_after(&args, "--sub-langs"), "en.*,vi.*,ko.*");
    assert!(args.contains(&"--yes-playlist".to_string()));
    assert!(!args.contains(&"--no-playlist".to_string()));
}
