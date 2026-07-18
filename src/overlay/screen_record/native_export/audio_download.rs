use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::json;

use super::audio_mix::{
    ExportAudioSource, IMPLICIT_AUDIO_EDGE_FADE_SEC, build_preprocessed_audio_mix,
    calculate_mix_output_duration,
};
use super::composition::{slice_audio_for_clip, slice_track_volume_points};
use super::config::{
    AudioDownloadClipJob, AudioDownloadConfig, AudioDownloadFormat, AudioDownloadTrackKind,
    DeviceAudioPoint, ImportedAudioSegmentConfig, SpeedPoint,
};

fn parse_json_with_path<T: DeserializeOwned>(args: serde_json::Value) -> Result<T, String> {
    let json = serde_json::to_string(&args).map_err(|e| format!("Serialize audio args: {e}"))?;
    let mut deserializer = serde_json::Deserializer::from_str(&json);
    serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|error| format!("{} at {}", error.inner(), error.path()))
}

fn output_base_dir(config: &AudioDownloadConfig) -> PathBuf {
    if config.output_dir.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(config.output_dir.trim())
    }
}

fn sanitize_file_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "Audio".to_string()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn default_volume_points(duration: f64, volume: f64) -> Vec<DeviceAudioPoint> {
    vec![
        DeviceAudioPoint { time: 0.0, volume },
        DeviceAudioPoint {
            time: duration.max(0.0),
            volume,
        },
    ]
}

fn sorted_speed_points(points: &[SpeedPoint]) -> Vec<SpeedPoint> {
    let mut out = points.to_vec();
    out.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn sorted_volume_points(
    points: &[DeviceAudioPoint],
    duration: f64,
    fallback: f64,
) -> Vec<DeviceAudioPoint> {
    let mut out = points.to_vec();
    out.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if out.is_empty() {
        default_volume_points(duration, fallback)
    } else {
        out
    }
}

fn push_imported_sources(
    sources: &mut Vec<ExportAudioSource>,
    segments: &[ImportedAudioSegmentConfig],
    track_volume_points: &[DeviceAudioPoint],
) {
    for audio_segment in segments {
        let path = audio_segment.raw_audio_path.trim();
        if path.is_empty() {
            continue;
        }
        let in_point = audio_segment.in_point.max(0.0);
        let raw_out = audio_segment.out_point;
        let out_point = if raw_out > in_point + 0.0001 {
            raw_out
        } else if audio_segment.duration > in_point + 0.0001 {
            audio_segment.duration
        } else {
            in_point
        };
        if out_point <= in_point + 0.0001 {
            continue;
        }
        let rate = if audio_segment.playback_rate > 0.0001 {
            audio_segment.playback_rate
        } else {
            1.0
        };
        sources.push(ExportAudioSource {
            path: path.to_string(),
            volume_points: track_volume_points.to_vec(),
            start_offset_sec: audio_segment.start_time - in_point / rate,
            source_in_sec: Some(in_point),
            source_out_sec: Some(out_point),
            playback_rate: rate,
            implicit_edge_fade_sec: 0.0,
        });
    }
}

fn build_clip_sources(
    config: &AudioDownloadConfig,
    clip: &AudioDownloadClipJob,
    project_clip_start_sec: f64,
) -> Vec<ExportAudioSource> {
    let mut sources = Vec::new();
    match config.track_kind {
        AudioDownloadTrackKind::Device => {
            let points =
                sorted_volume_points(&clip.segment.device_audio_points, clip.duration, 1.0);
            if points.iter().any(|point| point.volume > 0.0001) {
                let path = if clip.device_audio_path.trim().is_empty() {
                    clip.source_video_path.trim()
                } else {
                    clip.device_audio_path.trim()
                };
                if !path.is_empty() {
                    sources.push(ExportAudioSource {
                        path: path.to_string(),
                        volume_points: points,
                        start_offset_sec: clip.segment.device_audio_offset_sec,
                        source_in_sec: None,
                        source_out_sec: None,
                        playback_rate: 1.0,
                        implicit_edge_fade_sec: IMPLICIT_AUDIO_EDGE_FADE_SEC,
                    });
                }
            }
        }
        AudioDownloadTrackKind::Mic => {
            let points = sorted_volume_points(&clip.segment.mic_audio_points, clip.duration, 1.0);
            if !clip.mic_audio_path.trim().is_empty()
                && points.iter().any(|point| point.volume > 0.0001)
            {
                sources.push(ExportAudioSource {
                    path: clip.mic_audio_path.clone(),
                    volume_points: points,
                    start_offset_sec: clip.segment.mic_audio_offset_sec,
                    source_in_sec: None,
                    source_out_sec: None,
                    playback_rate: 1.0,
                    implicit_edge_fade_sec: IMPLICIT_AUDIO_EDGE_FADE_SEC,
                });
            }
        }
        AudioDownloadTrackKind::Imported => {
            let segments = slice_audio_for_clip(
                &config.audio_segments,
                project_clip_start_sec,
                clip.duration,
            );
            let volume = slice_track_volume_points(
                &config.audio_track_volume_points,
                project_clip_start_sec,
                clip.duration,
            );
            push_imported_sources(&mut sources, &segments, &volume);
        }
        AudioDownloadTrackKind::Narration => {
            let segments = slice_audio_for_clip(
                &config.narration_segments,
                project_clip_start_sec,
                clip.duration,
            );
            let volume = slice_track_volume_points(
                &config.narration_track_volume_points,
                project_clip_start_sec,
                clip.duration,
            );
            push_imported_sources(&mut sources, &segments, &volume);
        }
    }
    sources
}

fn concat_wavs(inputs: &[PathBuf], output: &Path) -> Result<(), String> {
    if inputs.is_empty() {
        return Err("No rendered audio clips to concatenate".to_string());
    }
    if inputs.len() == 1 {
        fs::copy(&inputs[0], output).map_err(|e| format!("Copy audio output: {e}"))?;
        return Ok(());
    }

    let mut first_reader =
        hound::WavReader::open(&inputs[0]).map_err(|e| format!("Open rendered WAV: {e}"))?;
    let spec = first_reader.spec();
    let mut writer = hound::WavWriter::create(output, spec)
        .map_err(|e| format!("Create concatenated WAV: {e}"))?;
    for sample in first_reader.samples::<i16>() {
        writer
            .write_sample(sample.map_err(|e| format!("Read rendered WAV sample: {e}"))?)
            .map_err(|e| format!("Write concatenated WAV sample: {e}"))?;
    }
    drop(first_reader);

    for input in inputs.iter().skip(1) {
        let mut reader =
            hound::WavReader::open(input).map_err(|e| format!("Open rendered WAV: {e}"))?;
        if reader.spec() != spec {
            return Err("Rendered WAV clips have incompatible formats".to_string());
        }
        for sample in reader.samples::<i16>() {
            writer
                .write_sample(sample.map_err(|e| format!("Read rendered WAV sample: {e}"))?)
                .map_err(|e| format!("Write concatenated WAV sample: {e}"))?;
        }
    }
    writer.finalize().map_err(|e| format!("Finalize WAV: {e}"))
}

fn write_silent_wav(output: &Path, duration_sec: f64) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: super::audio_mix::MIX_OUTPUT_CHANNELS as u16,
        sample_rate: super::audio_mix::MIX_OUTPUT_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(output, spec).map_err(|e| format!("Create silent WAV: {e}"))?;
    let frames = (duration_sec.max(0.0) * spec.sample_rate as f64).round() as usize;
    for _ in 0..frames {
        for _ in 0..spec.channels {
            writer
                .write_sample(0i16)
                .map_err(|e| format!("Write silent WAV sample: {e}"))?;
        }
    }
    writer
        .finalize()
        .map_err(|e| format!("Finalize silent WAV: {e}"))
}

fn audio_ffmpeg_download_message() -> String {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
        .tts_playground
        .screen_record_audio_ffmpeg_downloading
        .to_string()
}

fn encode_mp3(input_wav: &Path, output_mp3: &Path) -> Result<(), String> {
    let ffmpeg = crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge_message(
        &audio_ffmpeg_download_message(),
    )?;
    let output = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            input_wav.to_str().unwrap_or(""),
            "-vn",
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "192k",
            output_mp3.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| format!("Failed to launch FFmpeg MP3 export: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg MP3 export failed: {stderr}"));
    }
    Ok(())
}

pub fn start_audio_download(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = super::ExportActiveGuard::activate()?;
    super::EXPORT_CANCELLED.store(false, Ordering::SeqCst);

    let started = Instant::now();
    let config: AudioDownloadConfig = parse_json_with_path(args)?;
    if config.clips.is_empty() {
        return Err("Audio download has no clips".to_string());
    }

    let output_dir = output_base_dir(&config);
    fs::create_dir_all(&output_dir).map_err(|e| format!("Create output directory: {e}"))?;
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let track_label = sanitize_file_component(&config.track_label);
    let base_name = format!("SGT_{}_Audio_{}", track_label, timestamp_ms);
    let extension = match config.format {
        AudioDownloadFormat::Mp3 => "mp3",
        AudioDownloadFormat::Wav => "wav",
    };
    let final_path = output_dir.join(format!("{base_name}.{extension}"));
    let temp_root = crate::paths::app_local_data_dir()
        .join("audio-download")
        .join(timestamp_ms.to_string());
    fs::create_dir_all(&temp_root).map_err(|e| format!("Create audio temp dir: {e}"))?;

    let result = (|| -> Result<serde_json::Value, String> {
        let mut rendered_wavs = Vec::new();
        let mut audible_clip_count = 0usize;
        let mut project_clip_start = 0.0_f64;
        for (index, clip) in config.clips.iter().enumerate() {
            if super::EXPORT_CANCELLED.load(Ordering::SeqCst) {
                return Ok(json!({ "status": "cancelled" }));
            }
            eprintln!(
                "[AudioDownload] rendering clip {} ({}) for {:?}",
                clip.clip_name, clip.clip_id, config.track_kind
            );
            let sources = build_clip_sources(&config, clip, project_clip_start);
            project_clip_start += clip.duration.max(0.0);
            let speed_points = sorted_speed_points(&clip.segment.speed_points);
            if sources.is_empty() {
                let silent_duration = calculate_mix_output_duration(
                    clip.trim_start,
                    clip.duration,
                    &clip.segment.trim_segments,
                    &speed_points,
                );
                if silent_duration > 0.0001 {
                    let silent_path = temp_root.join(format!("{base_name}_silent_{index}.wav"));
                    write_silent_wav(&silent_path, silent_duration)?;
                    rendered_wavs.push(silent_path);
                }
                continue;
            }
            let file_stem = format!("{}_clip_{}", base_name, index);
            let Some(wav_path) = build_preprocessed_audio_mix(
                &sources,
                &speed_points,
                clip.trim_start,
                clip.duration,
                &clip.segment.trim_segments,
                &temp_root,
                &file_stem,
            )?
            else {
                continue;
            };
            audible_clip_count += 1;
            rendered_wavs.push(wav_path);
        }
        if rendered_wavs.is_empty() || audible_clip_count == 0 {
            return Err("Selected track has no audible audio to download".to_string());
        }

        let merged_wav = temp_root.join(format!("{base_name}_merged.wav"));
        concat_wavs(&rendered_wavs, &merged_wav)?;
        match config.format {
            AudioDownloadFormat::Wav => {
                fs::rename(&merged_wav, &final_path)
                    .or_else(|_| fs::copy(&merged_wav, &final_path).map(|_| ()))
                    .map_err(|e| format!("Move WAV output: {e}"))?;
            }
            AudioDownloadFormat::Mp3 => {
                encode_mp3(&merged_wav, &final_path)?;
            }
        }
        let bytes = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
        eprintln!(
            "[AudioDownload] wrote {} ({}) in {:.3}s",
            final_path.display(),
            extension,
            started.elapsed().as_secs_f64()
        );
        Ok(json!({
            "status": "success",
            "path": final_path.to_string_lossy(),
            "format": extension,
            "bytes": bytes,
            "trackKind": config.track_kind,
        }))
    })();

    if result.is_err() || super::EXPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = fs::remove_file(&final_path);
    }
    let _ = fs::remove_dir_all(&temp_root);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_audio_download_uses_track_delay() {
        let config = parse_json_with_path::<AudioDownloadConfig>(json!({
            "trackKind": "device",
            "clips": [{
                "clipId": "clip-1",
                "clipName": "Clip 1",
                "sourceVideoPath": "video.mp4",
                "deviceAudioPath": "device.wav",
                "trimStart": 0.0,
                "duration": 5.0,
                "segment": {
                    "crop": null,
                    "cursorVisibilitySegments": null,
                    "deviceAudioPoints": [
                        { "time": 0.0, "volume": 1.0 },
                        { "time": 5.0, "volume": 1.0 }
                    ],
                    "deviceAudioOffsetSec": 0.75
                }
            }]
        }))
        .expect("valid audio download config");

        let sources = build_clip_sources(&config, &config.clips[0], 0.0);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].start_offset_sec, 0.75);
    }
}
