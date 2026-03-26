use std::path::{Path, PathBuf};

/// Path to the FFmpeg binary installed by the app setup.
pub(super) fn ffmpeg_exe() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or(PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffmpeg.exe")
}

/// Convert a silent MP4 to an animated GIF using FFmpeg's two-pass palettegen+paletteuse.
///
/// Uses a single filtergraph with `split` so FFmpeg only decodes the input once.
/// The palette is built from the entire video (`stats_mode=full`) for best global quality.
/// Bayer dithering (scale 3) gives a good quality/size balance without Floyd-Steinberg noise.
pub(super) fn convert_mp4_to_gif(
    mp4_path: &Path,
    gif_path: &Path,
    max_width: u32,
) -> Result<(), String> {
    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return Err(format!(
            "FFmpeg not found at {}. Please install it via the app setup.",
            ffmpeg.display()
        ));
    }

    // scale=W:-1 keeps the original if the video is already narrower than max_width.
    let filter = format!(
        "scale='min({max_width},iw)':-1:flags=lanczos,\
         split[s0][s1];\
         [s0]palettegen=stats_mode=full[p];\
         [s1][p]paletteuse=dither=bayer:bayer_scale=3"
    );

    let out = std::process::Command::new(&ffmpeg)
        .args([
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
        .map_err(|e| format!("Failed to launch FFmpeg: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("FFmpeg GIF conversion failed:\n{stderr}"));
    }

    Ok(())
}
