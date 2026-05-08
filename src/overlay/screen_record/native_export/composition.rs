use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::json;

use super::config::{
    CompositionExportClipJob, CompositionExportConfig, ExportConfig, ImportedAudioSegmentConfig,
};
use super::native_stitch::{StitchClip, StitchConfig, stitch_clips_to_mp4};
use super::progress::{ExportProgressUpdate, push_export_progress_update};
use super::staging;

fn temp_export_root(session_id: &str) -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("screen-goated-toolbox")
        .join("composition-export")
        .join(session_id)
}

fn output_base_dir(config: &CompositionExportConfig) -> PathBuf {
    if config.output_dir.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(config.output_dir.trim())
    }
}

fn build_single_clip_config(
    export: &CompositionExportConfig,
    clip: &CompositionExportClipJob,
    temp_output_dir: &Path,
    project_clip_start_sec: f64,
) -> ExportConfig {
    ExportConfig {
        width: export.width,
        height: export.height,
        source_width: clip.source_width,
        source_height: clip.source_height,
        source_video_path: clip.source_video_path.clone(),
        framerate: export.framerate,
        target_video_bitrate_kbps: export.target_video_bitrate_kbps,
        quality_gate_percent: export.quality_gate_percent,
        pre_render_policy: export.pre_render_policy.clone(),
        device_audio_path: clip.device_audio_path.clone(),
        mic_audio_path: clip.mic_audio_path.clone(),
        webcam_video_path: clip.webcam_video_path.clone(),
        output_dir: temp_output_dir.to_string_lossy().to_string(),
        format: "mp4".to_string(),
        trim_start: clip.trim_start,
        duration: clip.duration,
        segment: clip.segment.clone(),
        background_config: clip.background_config.clone(),
        baked_path: None,
        baked_cursor_path: None,
        mouse_positions: clip.mouse_positions.clone(),
        audio_segments: slice_audio_for_clip(
            &export.audio_segments,
            project_clip_start_sec,
            clip.duration,
        ),
        audio_track_volume_points: slice_track_volume_points(
            &export.audio_track_volume_points,
            project_clip_start_sec,
            clip.duration,
        ),
        narration_segments: slice_audio_for_clip(
            &export.narration_segments,
            project_clip_start_sec,
            clip.duration,
        ),
        narration_track_volume_points: slice_track_volume_points(
            &export.narration_track_volume_points,
            project_clip_start_sec,
            clip.duration,
        ),
    }
}

/// Translate project-relative track volume points into clip-relative ones for
/// the clip occupying `[clip_start, clip_start + clip_duration]`.
fn slice_track_volume_points(
    points: &[super::config::DeviceAudioPoint],
    clip_start: f64,
    clip_duration: f64,
) -> Vec<super::config::DeviceAudioPoint> {
    if clip_duration <= 0.0 || points.is_empty() {
        return Vec::new();
    }
    fn volume_at(points: &[super::config::DeviceAudioPoint], time: f64) -> f64 {
        if points.is_empty() {
            return 1.0;
        }
        let idx = points.partition_point(|point| point.time < time);
        if idx == 0 {
            return points[0].volume;
        }
        if idx >= points.len() {
            return points.last().map(|point| point.volume).unwrap_or(1.0);
        }
        let left = &points[idx - 1];
        let right = &points[idx];
        let ratio = ((time - left.time) / (right.time - left.time).max(0.0001)).clamp(0.0, 1.0);
        let cos_t = (1.0 - (ratio * std::f64::consts::PI).cos()) / 2.0;
        left.volume + (right.volume - left.volume) * cos_t
    }

    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut sliced = Vec::new();
    sliced.push(super::config::DeviceAudioPoint {
        time: 0.0,
        volume: volume_at(&sorted, clip_start),
    });
    sliced.extend(sorted.iter().filter_map(|point| {
        let local = point.time - clip_start;
        if local <= 0.0001 || local >= clip_duration - 0.0001 {
            None
        } else {
            Some(super::config::DeviceAudioPoint {
                time: local,
                volume: point.volume,
            })
        }
    }));
    sliced.push(super::config::DeviceAudioPoint {
        time: clip_duration,
        volume: volume_at(&sorted, clip_start + clip_duration),
    });
    sliced
}

/// Convert project-relative audio segments to clip-relative ones for the
/// clip occupying `[clip_start, clip_start + clip_duration]` on the project
/// timeline. Audio segments fully outside the clip are dropped; partial
/// overlaps are trimmed to the clip range with adjusted in/out points.
fn slice_audio_for_clip(
    project_segments: &[ImportedAudioSegmentConfig],
    clip_start: f64,
    clip_duration: f64,
) -> Vec<ImportedAudioSegmentConfig> {
    if clip_duration <= 0.0 || project_segments.is_empty() {
        return Vec::new();
    }
    let clip_end = clip_start + clip_duration;
    let mut out = Vec::new();
    for seg in project_segments {
        let trimmed_len = (seg.out_point - seg.in_point).max(0.0);
        if trimmed_len <= 0.0 || seg.raw_audio_path.trim().is_empty() {
            continue;
        }
        let rate = if seg.playback_rate > 0.0001 {
            seg.playback_rate
        } else {
            1.0
        };
        let timeline_len = trimmed_len / rate;
        let seg_proj_start = seg.start_time;
        let seg_proj_end = seg_proj_start + timeline_len;
        let overlap_start = seg_proj_start.max(clip_start);
        let overlap_end = seg_proj_end.min(clip_end);
        if overlap_end - overlap_start <= 0.0001 {
            continue;
        }
        let local_start = overlap_start - clip_start;
        let in_offset = (overlap_start - seg_proj_start) * rate;
        let local_in = seg.in_point + in_offset.max(0.0);
        let local_out = local_in + (overlap_end - overlap_start) * rate;
        out.push(ImportedAudioSegmentConfig {
            raw_audio_path: seg.raw_audio_path.clone(),
            duration: seg.duration,
            start_time: local_start,
            in_point: local_in,
            out_point: local_out.min(seg.out_point),
            volume_points: seg.volume_points.clone(),
            playback_rate: seg.playback_rate,
        });
    }
    out
}

fn cleanup_file(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn start_composition_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = super::ExportActiveGuard::activate();
    super::EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    let export: CompositionExportConfig = parse_json_with_path(args)?;
    if export.clips.is_empty() {
        return Err("Composition export has no clips".to_string());
    }

    let temp_root = temp_export_root(&export.session_id);
    if temp_root.exists() {
        let _ = fs::remove_dir_all(&temp_root);
    }
    fs::create_dir_all(&temp_root)
        .map_err(|e| format!("Failed to create composition temp directory: {e}"))?;

    let final_dir = output_base_dir(&export);
    fs::create_dir_all(&final_dir)
        .map_err(|e| format!("Failed to create output directory: {e}"))?;

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let base_name = format!("SGT_Export_{timestamp_ms}");
    let final_mp4_path = final_dir.join(format!("{base_name}.mp4"));
    let final_gif_path = final_dir.join(format!("{base_name}.gif"));
    let merged_temp_mp4 = temp_root.join("composition_merged.mp4");

    let wants_gif = export.format == "gif" || export.format == "both";
    let render_phase_end = if wants_gif { 92.0 } else { 98.0 };
    let concat_phase_end = if wants_gif { 97.0 } else { 100.0 };
    let clip_count = export.clips.len() as u32;
    let mut rendered_clip_paths = Vec::with_capacity(export.clips.len());
    let mut rendered_clip_durations = Vec::with_capacity(export.clips.len());

    push_export_progress_update(ExportProgressUpdate {
        percent: 0.0,
        eta: 0.0,
        phase: Some("prepare"),
        clip_index: None,
        clip_count: Some(clip_count),
        clip_name: None,
    });

    let result = (|| -> Result<serde_json::Value, String> {
        // Track cumulative project time so each clip knows where it sits on
        // the global timeline and can receive its slice of the project-wide
        // audio track.
        let mut project_clip_start_sec = 0.0_f64;
        for (index, clip) in export.clips.iter().enumerate() {
            if !Path::new(&clip.source_video_path).exists() {
                return Err(format!(
                    "Clip \"{}\" ({}) is missing its source video at {}",
                    clip.clip_name, clip.clip_id, clip.source_video_path
                ));
            }

            let staged = staging::take_staged_for(&export.session_id, &clip.job_id);
            let temp_clip_config =
                build_single_clip_config(&export, clip, &temp_root, project_clip_start_sec);
            project_clip_start_sec += clip.duration.max(0.0);
            let clip_index = index as u32 + 1;
            let clip_name = clip.clip_name.clone();
            let render_start_pct = render_phase_end * index as f64 / clip_count as f64;
            let render_span_pct = render_phase_end / clip_count as f64;

            let render_result = super::run_native_export_with_staged(
                temp_clip_config,
                staged,
                0.0,
                Some(Box::new(move |pct, eta| {
                    push_export_progress_update(ExportProgressUpdate {
                        percent: render_start_pct
                            + (pct.clamp(0.0, 100.0) / 100.0) * render_span_pct,
                        eta,
                        phase: Some("render"),
                        clip_index: Some(clip_index),
                        clip_count: Some(clip_count),
                        clip_name: Some(clip_name.clone()),
                    });
                })),
            )?;

            if render_result["status"].as_str() == Some("cancelled") {
                return Ok(json!({ "status": "cancelled" }));
            }
            if super::EXPORT_CANCELLED.load(Ordering::SeqCst) {
                return Ok(json!({ "status": "cancelled" }));
            }

            let rendered_path = render_result["path"]
                .as_str()
                .ok_or("Composition clip render did not return an output path")?;
            rendered_clip_paths.push(PathBuf::from(rendered_path));
            rendered_clip_durations.push(clip.duration.max(0.0));
        }

        if super::EXPORT_CANCELLED.load(Ordering::SeqCst) {
            return Ok(json!({ "status": "cancelled" }));
        }

        push_export_progress_update(ExportProgressUpdate {
            percent: render_phase_end,
            eta: 0.0,
            phase: Some("concat"),
            clip_index: None,
            clip_count: Some(clip_count),
            clip_name: None,
        });

        let concat_target = if wants_gif && export.format == "gif" {
            &merged_temp_mp4
        } else {
            &final_mp4_path
        };
        let stitch_clips: Vec<StitchClip<'_>> = rendered_clip_paths
            .iter()
            .zip(rendered_clip_durations.iter())
            .map(|(path, duration_sec)| StitchClip {
                path,
                trim_start_sec: 0.0,
                duration_sec: *duration_sec,
            })
            .collect();
        stitch_clips_to_mp4(
            &stitch_clips,
            concat_target,
            &StitchConfig {
                width: export.width,
                height: export.height,
                framerate: export.framerate,
                bitrate_kbps: export.target_video_bitrate_kbps,
            },
        )?;
        for path in &rendered_clip_paths {
            cleanup_file(path);
        }

        if super::EXPORT_CANCELLED.load(Ordering::SeqCst) {
            return Ok(json!({ "status": "cancelled" }));
        }

        let mut artifacts = Vec::new();
        if export.format == "mp4" || export.format == "both" {
            let mp4_bytes = fs::metadata(&final_mp4_path).map(|m| m.len()).unwrap_or(0);
            artifacts.push(json!({
                "format": "mp4",
                "path": final_mp4_path.to_string_lossy(),
                "bytes": mp4_bytes,
                "primary": export.format != "gif",
            }));
        }

        if wants_gif {
            push_export_progress_update(ExportProgressUpdate {
                percent: concat_phase_end,
                eta: 0.0,
                phase: Some("gif"),
                clip_index: None,
                clip_count: Some(clip_count),
                clip_name: None,
            });
            let gif_source = if export.format == "gif" {
                &merged_temp_mp4
            } else {
                &final_mp4_path
            };
            super::gif::convert_mp4_to_gif(gif_source, &final_gif_path, export.width.min(960))?;
            let gif_bytes = fs::metadata(&final_gif_path).map(|m| m.len()).unwrap_or(0);
            artifacts.push(json!({
                "format": "gif",
                "path": final_gif_path.to_string_lossy(),
                "bytes": gif_bytes,
                "primary": export.format == "gif",
            }));
            if export.format == "gif" {
                cleanup_file(&merged_temp_mp4);
            }
        }

        let primary_path = if export.format == "gif" {
            final_gif_path.to_string_lossy().to_string()
        } else {
            final_mp4_path.to_string_lossy().to_string()
        };

        Ok(json!({
            "status": "success",
            "path": primary_path,
            "artifacts": artifacts,
        }))
    })();

    if result.is_err() || super::EXPORT_CANCELLED.load(Ordering::SeqCst) {
        cleanup_file(&final_mp4_path);
        cleanup_file(&final_gif_path);
        cleanup_file(&merged_temp_mp4);
    }
    let _ = fs::remove_dir_all(&temp_root);
    staging::clear_session(&export.session_id);

    result
}

fn parse_json_with_path<T: DeserializeOwned>(args: serde_json::Value) -> Result<T, String> {
    let json =
        serde_json::to_string(&args).map_err(|e| format!("Serialize composition args: {e}"))?;
    let mut deserializer = serde_json::Deserializer::from_str(&json);
    serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|error| format!("{} at {}", error.inner(), error.path()))
}
