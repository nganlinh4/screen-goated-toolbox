use std::path::Path;
use std::process::Command;
use std::time::Instant;

const FFMPEG_GIF_DITHER: &str = "bayer:bayer_scale=3";

/// Convert an MP4 to an animated GIF with the shared on-demand FFmpeg install.
pub(super) fn convert_mp4_to_gif(
    mp4_path: &Path,
    gif_path: &Path,
    max_width: u32,
) -> Result<(), String> {
    let started_at = Instant::now();
    let download_message = gif_ffmpeg_download_message();
    let ffmpeg = crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge_message(
        &download_message,
    )?;

    let max_width = max_width.max(1);
    let filter = format!(
        "scale='min({max_width},iw)':-1:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=full[p];[s1][p]paletteuse=dither={FFMPEG_GIF_DITHER}"
    );

    println!(
        "[FFmpegGif] Converting {} → {}",
        mp4_path.display(),
        gif_path.display()
    );
    let output = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            mp4_path.to_str().unwrap_or(""),
            "-vf",
            &filter,
            "-loop",
            "0",
            gif_path.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|err| format!("Failed to launch FFmpeg GIF export: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg GIF export failed: {stderr}"));
    }

    println!(
        "[FFmpegGif] Wrote GIF in {:.3}s → {}",
        started_at.elapsed().as_secs_f64(),
        gif_path.display()
    );
    Ok(())
}

fn gif_ffmpeg_download_message() -> String {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
        .tts_playground
        .screen_record_gif_ffmpeg_downloading
        .to_string()
}
