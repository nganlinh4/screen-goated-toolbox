use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::json;

use super::clip_audio_slice::{
    ClipSequenceWindow, slice_audio_for_clip, slice_track_volume_points,
};
use super::config::{
    CompositionExportClipJob, CompositionExportConfig, ExportConfig, ImportedAudioSegmentConfig,
};
use super::native_stitch::{StitchClip, StitchConfig, StitchObserver, stitch_clips_to_mp4};
use super::progress::{ExportProgressUpdate, push_export_progress_update};
use super::staging;

fn temp_export_root(session_id: &str) -> Result<(PathBuf, PathBuf), String> {
    super::validation::validate_identifier(session_id, "session id")?;
    let owner_root = crate::paths::app_runtime_local_data_dir().join("composition-export");
    let target = owner_root.join(session_id);
    if target.parent() != Some(owner_root.as_path()) {
        return Err("Composition temp directory escaped its owned root".to_string());
    }
    Ok((owner_root, target))
}

fn remove_temp_export_tree(owner_root: &Path, target: &Path) -> Result<(), String> {
    if target.parent() != Some(owner_root) || target.file_name().is_none() {
        return Err("Refusing to remove a directory outside the composition temp root".to_string());
    }
    match fs::remove_dir_all(target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove composition temp directory: {error}"
        )),
    }
}

fn output_base_dir(config: &CompositionExportConfig) -> PathBuf {
    if config.output_dir.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(config.output_dir.trim())
    }
}

fn rendered_duration(result: &serde_json::Value, framerate: u32) -> Result<f64, String> {
    let frames = result["frames"]
        .as_u64()
        .filter(|&frames| frames > 0)
        .ok_or("Composition clip render did not return a valid frame count")?;
    if framerate == 0 {
        return Err("Composition framerate must be positive".to_string());
    }
    Ok(frames as f64 / framerate as f64)
}

fn build_single_clip_config(
    export: &CompositionExportConfig,
    clip: &CompositionExportClipJob,
    temp_output_dir: &Path,
    project_clip_start_sec: f64,
) -> ExportConfig {
    // One clip keeps the project's source-time audio; a sequence slices its
    // compact-time tracks into this clip's source time.
    let window = ClipSequenceWindow::new(
        project_clip_start_sec,
        clip.trim_start,
        clip.duration,
        &clip.segment.trim_segments,
    );
    let sequence_audio = |segments: &[ImportedAudioSegmentConfig],
                          points: &[super::config::DeviceAudioPoint]| {
        if export.clips.len() == 1 {
            (segments.to_vec(), points.to_vec())
        } else {
            (
                slice_audio_for_clip(segments, &window),
                slice_track_volume_points(points, &window),
            )
        }
    };
    let (audio_segments, audio_track_volume_points) =
        sequence_audio(&export.audio_segments, &export.audio_track_volume_points);
    let (narration_segments, narration_track_volume_points) = sequence_audio(
        &export.narration_segments,
        &export.narration_track_volume_points,
    );
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
        audio_segments,
        audio_track_volume_points,
        narration_segments,
        narration_track_volume_points,
    }
}

fn cleanup_file(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn start_composition_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = super::ExportActiveGuard::activate()?;

    let export: CompositionExportConfig = parse_json_with_path(args)?;
    export.validate()?;

    let (temp_owner_root, temp_root) = temp_export_root(&export.session_id)?;
    if temp_root.exists() {
        remove_temp_export_tree(&temp_owner_root, &temp_root)?;
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
            if super::export_user_cancelled() {
                return Ok(json!({ "status": "cancelled" }));
            }

            let rendered_path = render_result["path"]
                .as_str()
                .ok_or("Composition clip render did not return an output path")?;
            rendered_clip_paths.push(PathBuf::from(rendered_path));
            rendered_clip_durations.push(rendered_duration(&render_result, export.framerate)?);
        }

        if super::export_user_cancelled() {
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
        let report_stitch_progress = |fraction: f64| {
            push_export_progress_update(ExportProgressUpdate {
                percent: render_phase_end + (concat_phase_end - render_phase_end) * fraction,
                eta: 0.0,
                phase: Some("concat"),
                clip_index: None,
                clip_count: Some(clip_count),
                clip_name: None,
            });
        };
        let stitched = stitch_clips_to_mp4(
            &stitch_clips,
            concat_target,
            &StitchConfig {
                width: export.width,
                height: export.height,
                framerate: export.framerate,
                bitrate_kbps: export.target_video_bitrate_kbps,
            },
            &StitchObserver {
                cancelled: &super::export_user_cancelled,
                progress: &report_stitch_progress,
            },
        );
        stitched?;
        for path in &rendered_clip_paths {
            cleanup_file(path);
        }

        if super::export_user_cancelled() {
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

    if result.is_err() || super::export_user_cancelled() {
        cleanup_file(&final_mp4_path);
        cleanup_file(&final_gif_path);
        cleanup_file(&merged_temp_mp4);
    }
    if let Err(error) = remove_temp_export_tree(&temp_owner_root, &temp_root) {
        eprintln!("[CompositionExport] {error}");
    }
    staging::clear_session(&export.session_id);

    settle_composition_result(result, super::export_user_cancelled())
}

/// A composition that unwound with an error (a clip render, the stitch, GIF
/// conversion) is classified like a single-clip export: it is a cancellation
/// only when the user asked for one, never because an internal failure raised
/// the shared abort flag.
fn settle_composition_result(
    result: Result<serde_json::Value, String>,
    user_cancelled: bool,
) -> Result<serde_json::Value, String> {
    result.or_else(|error| super::pipeline_build::classify_unwound_export(error, user_cancelled))
}

fn parse_json_with_path<T: DeserializeOwned>(args: serde_json::Value) -> Result<T, String> {
    let json =
        serde_json::to_string(&args).map_err(|e| format!("Serialize composition args: {e}"))?;
    let mut deserializer = serde_json::Deserializer::from_str(&json);
    serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|error| format!("{} at {}", error.inner(), error.path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stitch_failures_are_errors_unless_the_user_cancelled() {
        let cancelled_stitch = Err(super::super::native_stitch::STITCH_CANCELLED.to_string());
        assert_eq!(
            settle_composition_result(cancelled_stitch, true).unwrap()["status"],
            "cancelled"
        );
        let encode_failure = Err("Stitch encoder rejected frame 12".to_string());
        assert_eq!(
            settle_composition_result(encode_failure, false),
            Err("Stitch encoder rejected frame 12".to_string())
        );
        let finished = Ok(json!({ "status": "success", "path": "out.mp4" }));
        assert_eq!(
            settle_composition_result(finished, true).unwrap()["status"],
            "success"
        );
    }

    #[test]
    fn both_format_single_clip_keeps_source_time_audio() {
        let export: CompositionExportConfig = serde_json::from_value(json!({
            "sessionId": "single", "width": 100, "height": 100,
            "framerate": 30, "format": "both", "clips": [{
                "jobId": "one", "clipId": "one", "clipName": "One",
                "sourceVideoPath": "video.mp4", "trimStart": 8, "duration": 2,
                "segment": {"crop": null, "cursorVisibilitySegments": null,
                    "trimSegments": [{"startTime": 8, "endTime": 10}]},
                "backgroundConfig": {"scale": 100, "borderRadius": 0,
                    "backgroundType": "solid", "shadow": 0, "cursorScale": 1}
            }],
            "audioSegments": [{"rawAudioPath": "music.wav", "duration": 1,
                "startTime": 8, "inPoint": 0, "outPoint": 1}],
            "audioTrackVolumePoints": [{"time": 8, "volume": 0.4}],
            "narrationSegments": [{"rawAudioPath": "speech.wav", "duration": 1,
                "startTime": 9, "inPoint": 0, "outPoint": 1}],
            "narrationTrackVolumePoints": [{"time": 9, "volume": 0.6}]
        }))
        .unwrap();
        let clip = build_single_clip_config(&export, &export.clips[0], Path::new("."), 0.0);
        assert_eq!(clip.audio_segments.len(), 1);
        assert_eq!(clip.audio_segments[0].start_time, 8.0);
        assert_eq!(clip.audio_track_volume_points[0].time, 8.0);
        assert_eq!(clip.narration_segments.len(), 1);
        assert_eq!(clip.narration_segments[0].start_time, 9.0);
        assert_eq!(clip.narration_track_volume_points[0].time, 9.0);
    }

    #[test]
    fn sequence_audio_lands_at_the_clip_source_time_it_plays_over() {
        // Clip two keeps source 3..5 and 7..9: sequence 2..4 and 4..6. Music
        // at sequence 2.5..5.5 plays over source 3.5..5 and then 7..8.5.
        let clip = |id: &str, trims: serde_json::Value, trim_start: f64, duration: f64| {
            json!({"jobId": id, "clipId": id, "clipName": id,
                "sourceVideoPath": "video.mp4", "trimStart": trim_start, "duration": duration,
                "segment": {"crop": null, "cursorVisibilitySegments": null, "trimSegments": trims},
                "backgroundConfig": {"scale": 100, "borderRadius": 0,
                    "backgroundType": "solid", "shadow": 0, "cursorScale": 1}})
        };
        let export: CompositionExportConfig = serde_json::from_value(json!({
            "sessionId": "sequence", "width": 100, "height": 100, "framerate": 30, "format": "mp4",
            "clips": [
                clip("one", json!([{"startTime": 0, "endTime": 2}]), 0.0, 2.0),
                clip("two", json!([{"startTime": 3, "endTime": 5},
                    {"startTime": 7, "endTime": 9}]), 3.0, 4.0),
            ],
            "audioSegments": [{"rawAudioPath": "music.wav", "duration": 3,
                "startTime": 2.5, "inPoint": 0, "outPoint": 3}],
            "audioTrackVolumePoints": [{"time": 2, "volume": 0.2},
                {"time": 4.5, "volume": 0.8}, {"time": 6, "volume": 0.8}]
        }))
        .unwrap();
        let config = build_single_clip_config(&export, &export.clips[1], Path::new("."), 2.0);
        let pieces: Vec<(f64, f64, f64)> = config
            .audio_segments
            .iter()
            .map(|piece| (piece.start_time, piece.in_point, piece.out_point))
            .collect();
        assert_eq!(pieces, [(3.5, 0.0, 1.5), (7.0, 1.5, 3.0)]);
        let volume = &config.audio_track_volume_points;
        assert_eq!(volume.first().map(|point| point.time), Some(3.0));
        assert!(
            volume
                .iter()
                .any(|point| point.time == 7.5 && point.volume == 0.8),
            "the sequence 4.5 s point plays at source 7.5 s"
        );
    }

    #[test]
    fn stitch_durations_follow_rendered_frames() {
        // Both clips cover ten seconds of source, but their rendered lengths
        // differ when playback speed changes.
        let slow = rendered_duration(&json!({ "frames": 600 }), 30).unwrap();
        let fast = rendered_duration(&json!({ "frames": 150 }), 30).unwrap();
        assert_eq!(slow, 20.0);
        assert_eq!(fast, 5.0);
        assert_eq!(((slow + fast) * 30.0).round() as u64, 750);
    }

    #[test]
    fn stitch_duration_rejects_missing_or_invalid_render_metadata() {
        assert!(rendered_duration(&json!({}), 30).is_err());
        assert!(rendered_duration(&json!({ "frames": 0 }), 30).is_err());
        assert!(rendered_duration(&json!({ "frames": 1 }), 0).is_err());
    }
}
