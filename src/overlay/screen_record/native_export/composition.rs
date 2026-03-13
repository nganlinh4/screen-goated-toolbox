use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::config::{CompositionExportClipJob, CompositionExportConfig, ExportConfig};
use super::progress::{ExportProgressUpdate, push_export_progress_update};
use super::staging;

const NORMALIZED_AUDIO_SAMPLE_RATE: u32 = 48_000;
const NORMALIZED_AUDIO_CHANNELS: u32 = 2;
const NORMALIZED_AUDIO_BITRATE_KBPS: u32 = 192;

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

fn ffmpeg_exe() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or(PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffmpeg.exe")
}

fn ffprobe_exe() -> PathBuf {
    ffmpeg_exe().with_file_name("ffprobe.exe")
}

fn media_has_audio(path: &Path) -> bool {
    let ffprobe = ffprobe_exe();
    let path_str = path.to_string_lossy().to_string();
    if ffprobe.exists()
        && let Ok(output) = Command::new(&ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
                &path_str,
            ])
            .output()
        && output.status.success()
    {
        return !String::from_utf8_lossy(&output.stdout).trim().is_empty();
    }

    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return false;
    }

    Command::new(&ffmpeg)
        .args(["-i", &path_str])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stderr).contains("Audio:"))
        .unwrap_or(false)
}

fn normalize_concat_clip(input_path: &Path, output_path: &Path) -> Result<(), String> {
    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return Err(format!(
            "FFmpeg not found at {}. Please install it via the app setup.",
            ffmpeg.display()
        ));
    }

    let input_path_str = input_path.to_string_lossy().to_string();
    let output_path_str = output_path.to_string_lossy().to_string();
    let sample_rate = NORMALIZED_AUDIO_SAMPLE_RATE.to_string();
    let channels = NORMALIZED_AUDIO_CHANNELS.to_string();
    let audio_bitrate = format!("{NORMALIZED_AUDIO_BITRATE_KBPS}k");
    let mut command = Command::new(&ffmpeg);
    command.arg("-y").arg("-i").arg(&input_path_str);
    if media_has_audio(input_path) {
        command.args([
            "-map",
            "0:v:0",
            "-map",
            "0:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-ar",
            &sample_rate,
            "-ac",
            &channels,
            "-b:a",
            &audio_bitrate,
            &output_path_str,
        ]);
    } else {
        let null_audio =
            format!("anullsrc=channel_layout=stereo:sample_rate={NORMALIZED_AUDIO_SAMPLE_RATE}");
        command.args([
            "-f",
            "lavfi",
            "-i",
            &null_audio,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            &audio_bitrate,
            "-shortest",
            &output_path_str,
        ]);
    }

    let output = command
        .output()
        .map_err(|e| format!("Failed to launch FFmpeg: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "FFmpeg audio normalization failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn concat_mp4s(inputs: &[PathBuf], output_path: &Path) -> Result<(), String> {
    let ffmpeg = ffmpeg_exe();
    if !ffmpeg.exists() {
        return Err(format!(
            "FFmpeg not found at {}. Please install it via the app setup.",
            ffmpeg.display()
        ));
    }
    if inputs.is_empty() {
        return Err("No rendered clips to concatenate".to_string());
    }

    let list_path = output_path.with_extension("concat.txt");
    let mut list_contents = String::new();
    for input in inputs {
        let escaped = input.to_string_lossy().replace('\'', "'\\''");
        list_contents.push_str(&format!("file '{}'\n", escaped));
    }
    fs::write(&list_path, list_contents)
        .map_err(|e| format!("Failed to write concat list: {e}"))?;
    let list_path_str = list_path.to_string_lossy().to_string();
    let output_path_str = output_path.to_string_lossy().to_string();

    let output = Command::new(&ffmpeg)
        .args([
            "-y",
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
        .map_err(|e| format!("Failed to launch FFmpeg concat: {e}"))?;
    let _ = fs::remove_file(&list_path);

    if !output.status.success() {
        return Err(format!(
            "FFmpeg concat failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn build_single_clip_config(
    export: &CompositionExportConfig,
    clip: &CompositionExportClipJob,
    temp_output_dir: &Path,
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
        output_dir: temp_output_dir.to_string_lossy().to_string(),
        format: "mp4".to_string(),
        trim_start: clip.trim_start,
        duration: clip.duration,
        segment: clip.segment.clone(),
        background_config: clip.background_config.clone(),
        baked_path: None,
        baked_cursor_path: None,
        mouse_positions: clip.mouse_positions.clone(),
    }
}

fn cleanup_file(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn start_composition_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = super::ExportActiveGuard::activate();
    super::EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    let export: CompositionExportConfig =
        serde_json::from_value(args).map_err(|e| e.to_string())?;
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
    let mut concat_ready_paths = Vec::with_capacity(export.clips.len());

    push_export_progress_update(ExportProgressUpdate {
        percent: 0.0,
        eta: 0.0,
        phase: Some("prepare"),
        clip_index: None,
        clip_count: Some(clip_count),
        clip_name: None,
    });

    let result = (|| -> Result<serde_json::Value, String> {
        for (index, clip) in export.clips.iter().enumerate() {
            if !Path::new(&clip.source_video_path).exists() {
                return Err(format!(
                    "Clip \"{}\" ({}) is missing its source video at {}",
                    clip.clip_name, clip.clip_id, clip.source_video_path
                ));
            }

            let staged = staging::take_staged_for(&export.session_id, &clip.job_id);
            let temp_clip_config = build_single_clip_config(&export, clip, &temp_root);
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
            let normalized_path =
                temp_root.join(format!("clip_{clip_index:03}_{}_norm.mp4", clip.clip_id));
            normalize_concat_clip(Path::new(rendered_path), &normalized_path)?;
            cleanup_file(Path::new(rendered_path));
            concat_ready_paths.push(normalized_path);
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
        concat_mp4s(&concat_ready_paths, concat_target)?;

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
            super::convert_mp4_to_gif(gif_source, &final_gif_path, export.width.min(960))?;
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
