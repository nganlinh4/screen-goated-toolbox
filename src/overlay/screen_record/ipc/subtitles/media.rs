use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::audio::build_trimmed_wav;
use super::types::{SubtitleClipRequest, SubtitleGenerationMethod, SubtitleTrimSegment};

const GEMINI_VIDEO_AUDIO_BITRATE: &str = "96k";
const GEMINI_VIDEO_CRF: &str = "30";
const GEMINI_VIDEO_PRESET: &str = "veryfast";

pub struct PreparedSubtitleMedia {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub file_name: String,
    pub duration_sec: f64,
}

pub fn prepare_clip_media(
    method: SubtitleGenerationMethod,
    source_type: &str,
    clip: &SubtitleClipRequest,
) -> Result<PreparedSubtitleMedia, String> {
    let duration_sec = compact_duration_sec(&clip.trim_segments);
    if matches!(
        method,
        SubtitleGenerationMethod::Gemini3_1FlashLite
            | SubtitleGenerationMethod::Gemini3FlashPreview
    ) && source_type == "video"
    {
        let bytes = build_trimmed_video_mp4_bytes(&clip.source_path, &clip.trim_segments)?;
        return Ok(PreparedSubtitleMedia {
            bytes,
            mime_type: "video/mp4".to_string(),
            file_name: "subtitle-source.mp4".to_string(),
            duration_sec,
        });
    }

    let bytes = build_trimmed_wav(
        &clip.source_path,
        &clip.trim_segments,
        clip.mic_audio_offset_sec.unwrap_or(0.0),
        source_type == "mic",
    )?;
    Ok(PreparedSubtitleMedia {
        bytes,
        mime_type: "audio/wav".to_string(),
        file_name: "subtitle-source.wav".to_string(),
        duration_sec,
    })
}

fn compact_duration_sec(trim_segments: &[SubtitleTrimSegment]) -> f64 {
    trim_segments
        .iter()
        .map(|segment| (segment.end_time - segment.start_time).max(0.0))
        .sum::<f64>()
}

fn build_trimmed_video_mp4_bytes(
    source_path: &str,
    trim_segments: &[SubtitleTrimSegment],
) -> Result<Vec<u8>, String> {
    if trim_segments.is_empty() {
        return Err("Gemini subtitle video source had no trim segments".to_string());
    }

    let temp_root = std::env::temp_dir()
        .join("screen-goated-toolbox")
        .join("subtitle-media")
        .join(unique_suffix());
    fs::create_dir_all(&temp_root)
        .map_err(|err| format!("Create subtitle video temp dir: {err}"))?;

    let result = build_trimmed_video_mp4_bytes_inner(source_path, trim_segments, &temp_root);
    let _ = fs::remove_dir_all(&temp_root);
    result
}

fn build_trimmed_video_mp4_bytes_inner(
    source_path: &str,
    trim_segments: &[SubtitleTrimSegment],
    temp_root: &Path,
) -> Result<Vec<u8>, String> {
    let source = Path::new(source_path);
    let final_output = temp_root.join("subtitle-source.mp4");

    if trim_segments.len() == 1 {
        render_trimmed_video_part(source, &trim_segments[0], &final_output)?;
        return fs::read(&final_output).map_err(|err| format!("Read subtitle video bytes: {err}"));
    }

    let mut parts = Vec::with_capacity(trim_segments.len());
    for (index, trim_segment) in trim_segments.iter().enumerate() {
        let part_path = temp_root.join(format!("part-{index:03}.mp4"));
        render_trimmed_video_part(source, trim_segment, &part_path)?;
        parts.push(part_path);
    }
    concat_trimmed_video_parts(&parts, &final_output)?;
    fs::read(&final_output).map_err(|err| format!("Read concatenated subtitle video bytes: {err}"))
}

fn render_trimmed_video_part(
    source_path: &Path,
    trim_segment: &SubtitleTrimSegment,
    output_path: &Path,
) -> Result<(), String> {
    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return Err(format!(
            "FFmpeg not found at {}. Please install it via the app setup.",
            ffmpeg.display()
        ));
    }

    let start = trim_segment.start_time.max(0.0);
    let duration = (trim_segment.end_time - trim_segment.start_time).max(0.0);
    if duration <= 0.0 {
        return Err("Subtitle video trim segment had no duration".to_string());
    }

    let source_path_str = source_path.to_string_lossy().to_string();
    let output_path_str = output_path.to_string_lossy().to_string();
    let output = Command::new(&ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{start:.6}"),
            "-t",
            &format!("{duration:.6}"),
            "-i",
            &source_path_str,
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c:v",
            "libx264",
            "-preset",
            GEMINI_VIDEO_PRESET,
            "-crf",
            GEMINI_VIDEO_CRF,
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            GEMINI_VIDEO_AUDIO_BITRATE,
            "-movflags",
            "+faststart",
            &output_path_str,
        ])
        .output()
        .map_err(|err| format!("Launch FFmpeg for subtitle video trim: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "FFmpeg subtitle video trim failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn concat_trimmed_video_parts(parts: &[PathBuf], output_path: &Path) -> Result<(), String> {
    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return Err(format!(
            "FFmpeg not found at {}. Please install it via the app setup.",
            ffmpeg.display()
        ));
    }

    let list_path = output_path.with_extension("concat.txt");
    let mut list_contents = String::new();
    for part in parts {
        let escaped = part.to_string_lossy().replace('\'', "'\\''");
        list_contents.push_str(&format!("file '{escaped}'\n"));
    }
    fs::write(&list_path, list_contents)
        .map_err(|err| format!("Write subtitle concat list: {err}"))?;

    let list_path_str = list_path.to_string_lossy().to_string();
    let output_path_str = output_path.to_string_lossy().to_string();
    let output = Command::new(&ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &list_path_str,
            "-c",
            "copy",
            &output_path_str,
        ])
        .output()
        .map_err(|err| format!("Launch FFmpeg subtitle concat: {err}"))?;
    let _ = fs::remove_file(&list_path);

    if !output.status.success() {
        return Err(format!(
            "FFmpeg subtitle concat failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn ffmpeg_exe() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffmpeg.exe")
}

fn unique_suffix() -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("subtitle-{}-{}", timestamp_ms, std::process::id())
}
