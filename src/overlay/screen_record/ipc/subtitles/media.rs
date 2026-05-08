use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::overlay::screen_record::mf_decode;
use crate::overlay::screen_record::native_export::config::compute_default_video_bitrate_kbps;
use crate::overlay::screen_record::native_export::native_stitch::{
    StitchClip, StitchConfig, stitch_clips_to_mp4,
};

use super::audio::build_trimmed_wav;
use super::types::{SubtitleClipRequest, SubtitleGenerationMethod, SubtitleTrimSegment};

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
    let final_output = temp_root.join("subtitle-source.mp4");
    let metadata = mf_decode::probe_video_metadata(source_path)?;
    let framerate = if metadata.fps_num > 0 && metadata.fps_den > 0 {
        ((metadata.fps_num as f64 / metadata.fps_den as f64).round() as u32).max(1)
    } else {
        30
    };
    let bitrate_kbps =
        compute_default_video_bitrate_kbps(metadata.width, metadata.height, framerate);
    let source = Path::new(source_path);
    let clips: Vec<StitchClip<'_>> = trim_segments
        .iter()
        .filter_map(|segment| {
            let duration_sec = (segment.end_time - segment.start_time).max(0.0);
            (duration_sec > 0.0).then_some(StitchClip {
                path: source,
                trim_start_sec: segment.start_time.max(0.0),
                duration_sec,
            })
        })
        .collect();
    stitch_clips_to_mp4(
        &clips,
        &final_output,
        &StitchConfig {
            width: metadata.width,
            height: metadata.height,
            framerate,
            bitrate_kbps,
        },
    )?;
    fs::read(&final_output).map_err(|err| format!("Read concatenated subtitle video bytes: {err}"))
}

fn unique_suffix() -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("subtitle-{}-{}", timestamp_ms, std::process::id())
}
