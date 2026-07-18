use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::overlay::screen_record::engine::VIDEO_PATH;

use super::super::audio_mix::{
    ExportAudioSource, IMPLICIT_AUDIO_EDGE_FADE_SEC, build_preprocessed_audio_mix,
};
use super::super::config::{self, ExportConfig, ImportedAudioSegmentConfig};

pub(super) type AudioVideoPrep = (
    String,
    Option<String>,
    Option<PathBuf>,
    bool,
    Vec<config::DeviceAudioPoint>,
    Vec<config::SpeedPoint>,
);

pub(super) fn prepare_audio_and_video(config: &ExportConfig) -> Result<AudioVideoPrep, String> {
    let explicit_source_video_path = config.source_video_path.trim().to_string();
    let source_video_path = if !explicit_source_video_path.is_empty()
        && Path::new(&explicit_source_video_path).exists()
    {
        explicit_source_video_path
    } else {
        VIDEO_PATH
            .lock()
            .unwrap()
            .clone()
            .ok_or("No source video found")?
    };

    let legacy_audio_volume = config.background_config.volume.clamp(0.0, 1.0);
    let mut device_audio_points = config.segment.device_audio_points.clone();
    device_audio_points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if device_audio_points.is_empty() {
        device_audio_points = vec![
            config::DeviceAudioPoint {
                time: 0.0,
                volume: legacy_audio_volume,
            },
            config::DeviceAudioPoint {
                time: config.duration.max(0.0),
                volume: legacy_audio_volume,
            },
        ];
    }
    let mut mic_audio_points = config.segment.mic_audio_points.clone();
    mic_audio_points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if mic_audio_points.is_empty() {
        mic_audio_points = vec![
            config::DeviceAudioPoint {
                time: 0.0,
                volume: 0.0,
            },
            config::DeviceAudioPoint {
                time: config.duration.max(0.0),
                volume: 0.0,
            },
        ];
    }
    let has_audible_device_audio = device_audio_points
        .iter()
        .any(|point| point.volume > 0.0001);
    let has_audible_mic_audio = mic_audio_points.iter().any(|point| point.volume > 0.0001);
    let mut speed_points = config.segment.speed_points.clone();
    speed_points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let output_base_dir = if config.output_dir.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(config.output_dir.trim())
    };

    let speed_changes_audio_timeline = speed_points
        .iter()
        .any(|point| (point.speed - 1.0).abs() > 0.0001);
    let has_audio_segments = config
        .audio_segments
        .iter()
        .any(|seg| !seg.raw_audio_path.trim().is_empty());
    let has_narration_segments = config
        .narration_segments
        .iter()
        .any(|seg| !seg.raw_audio_path.trim().is_empty());
    let t_audio_start = Instant::now();
    let use_preprocessed_audio = config.format != "gif"
        && (speed_changes_audio_timeline
            || (!config.mic_audio_path.trim().is_empty() && has_audible_mic_audio)
            || has_audio_segments
            || has_narration_segments);
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let mixed_audio_path = if use_preprocessed_audio {
        build_mixed_audio(MixedAudioBuild {
            config,
            device_audio_points: &device_audio_points,
            mic_audio_points: &mic_audio_points,
            speed_points: &speed_points,
            has_audible_device_audio,
            has_audible_mic_audio,
            output_base_dir: &output_base_dir,
            timestamp_ms,
        })?
    } else {
        None
    };
    let mixed_audio_cleanup_path = mixed_audio_path.clone();
    let source_audio_path = if let Some(path) = &mixed_audio_path {
        Some(path.to_string_lossy().to_string())
    } else if !config.device_audio_path.is_empty()
        && has_audible_device_audio
        && config.format != "gif"
    {
        Some(config.device_audio_path.clone())
    } else {
        None
    };
    let audio_is_preprocessed = mixed_audio_path.is_some();
    eprintln!(
        "[Export][Timing] Audio preprocessing: {:.3}s (preprocessed={})",
        t_audio_start.elapsed().as_secs_f64(),
        audio_is_preprocessed
    );

    Ok((
        source_video_path,
        source_audio_path,
        mixed_audio_cleanup_path,
        audio_is_preprocessed,
        device_audio_points,
        speed_points,
    ))
}

struct MixedAudioBuild<'a> {
    config: &'a ExportConfig,
    device_audio_points: &'a [config::DeviceAudioPoint],
    mic_audio_points: &'a [config::DeviceAudioPoint],
    speed_points: &'a [config::SpeedPoint],
    has_audible_device_audio: bool,
    has_audible_mic_audio: bool,
    output_base_dir: &'a Path,
    timestamp_ms: u128,
}

fn build_mixed_audio(ctx: MixedAudioBuild<'_>) -> Result<Option<PathBuf>, String> {
    let mut sources = Vec::new();
    let config = ctx.config;
    if !config.device_audio_path.trim().is_empty() && ctx.has_audible_device_audio {
        sources.push(ExportAudioSource {
            path: config.device_audio_path.clone(),
            volume_points: ctx.device_audio_points.to_vec(),
            start_offset_sec: config.segment.device_audio_offset_sec,
            source_in_sec: None,
            source_out_sec: None,
            playback_rate: 1.0,
            implicit_edge_fade_sec: IMPLICIT_AUDIO_EDGE_FADE_SEC,
        });
    }
    if !config.mic_audio_path.trim().is_empty() && ctx.has_audible_mic_audio {
        sources.push(ExportAudioSource {
            path: config.mic_audio_path.clone(),
            volume_points: ctx.mic_audio_points.to_vec(),
            start_offset_sec: config.segment.mic_audio_offset_sec,
            source_in_sec: None,
            source_out_sec: None,
            playback_rate: 1.0,
            implicit_edge_fade_sec: IMPLICIT_AUDIO_EDGE_FADE_SEC,
        });
    }
    push_clip_sources(
        &mut sources,
        &config.audio_segments,
        &config.audio_track_volume_points,
    );
    push_clip_sources(
        &mut sources,
        &config.narration_segments,
        &config.narration_track_volume_points,
    );
    if sources.is_empty() {
        return Ok(None);
    }
    build_preprocessed_audio_mix(
        &sources,
        ctx.speed_points,
        config.trim_start,
        config.duration,
        &config.segment.trim_segments,
        ctx.output_base_dir,
        &format!("SGT_Export_{}", ctx.timestamp_ms),
    )
}

fn push_clip_sources(
    sources: &mut Vec<ExportAudioSource>,
    segments: &[ImportedAudioSegmentConfig],
    track_volume_points: &[config::DeviceAudioPoint],
) {
    for audio_segment in segments {
        let path = audio_segment.raw_audio_path.trim();
        if path.is_empty() {
            continue;
        }
        // The mix maps source time -> project time as
        //   project_t = (source_t / rate) + (start_time - in_point)
        // so the trimmed range [in_point, out_point] is placed at
        // [start_time, start_time + (out_point - in_point) / rate].
        let in_point = audio_segment.in_point.max(0.0);
        let raw_out = audio_segment.out_point;
        let out_point = if raw_out > in_point + 0.0001 {
            raw_out
        } else if audio_segment.duration > in_point + 0.0001 {
            audio_segment.duration
        } else {
            in_point
        };
        let rate = if audio_segment.playback_rate > 0.0001 {
            audio_segment.playback_rate
        } else {
            1.0
        };
        sources.push(ExportAudioSource {
            path: path.to_string(),
            // Track-global volume envelope: every source on this track
            // inherits the same project-time curve.
            volume_points: track_volume_points.to_vec(),
            start_offset_sec: audio_segment.start_time - in_point / rate,
            source_in_sec: Some(in_point),
            source_out_sec: Some(out_point),
            playback_rate: rate,
            implicit_edge_fade_sec: 0.0,
        });
    }
}
