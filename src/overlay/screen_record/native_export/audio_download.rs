use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::json;

use super::audio_mix::{
    ExportAudioSource, IMPLICIT_AUDIO_EDGE_FADE_SEC, build_preprocessed_audio_mix,
    calculate_mix_output_duration,
};
use super::clip_audio_slice::{
    ClipSequenceWindow, slice_audio_for_clip, slice_track_volume_points,
};
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
            // An empty device path means the clip has no device audio; the
            // clip still occupies its duration as silence.
            let points =
                sorted_volume_points(&clip.segment.device_audio_points, clip.duration, 1.0);
            if points.iter().any(|point| point.volume > 0.0001) {
                let path = clip.device_audio_path.trim();
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
            if config.clips.len() == 1 {
                push_imported_sources(
                    &mut sources,
                    &config.audio_segments,
                    &config.audio_track_volume_points,
                );
                return sources;
            }
            let window = clip_sequence_window(clip, project_clip_start_sec);
            let segments = slice_audio_for_clip(&config.audio_segments, &window);
            let volume = slice_track_volume_points(&config.audio_track_volume_points, &window);
            push_imported_sources(&mut sources, &segments, &volume);
        }
        AudioDownloadTrackKind::Narration => {
            if config.clips.len() == 1 {
                push_imported_sources(
                    &mut sources,
                    &config.narration_segments,
                    &config.narration_track_volume_points,
                );
                return sources;
            }
            let window = clip_sequence_window(clip, project_clip_start_sec);
            let segments = slice_audio_for_clip(&config.narration_segments, &window);
            let volume = slice_track_volume_points(&config.narration_track_volume_points, &window);
            push_imported_sources(&mut sources, &segments, &volume);
        }
    }
    sources
}

fn clip_sequence_window(clip: &AudioDownloadClipJob, sequence_start: f64) -> ClipSequenceWindow {
    ClipSequenceWindow::new(
        sequence_start,
        clip.trim_start,
        clip.duration,
        &clip.segment.trim_segments,
    )
}

fn render_clip_audio(
    sources: &[ExportAudioSource],
    clip: &AudioDownloadClipJob,
    speed_points: &[SpeedPoint],
    temp_root: &Path,
    file_stem: &str,
) -> Result<Option<(PathBuf, bool)>, String> {
    if let Some(path) = build_preprocessed_audio_mix(
        sources,
        speed_points,
        clip.trim_start,
        clip.duration,
        &clip.segment.trim_segments,
        temp_root,
        file_stem,
    )? {
        return Ok(Some((path, true)));
    }
    // A silent/missing source still occupies this clip's output time. Dropping
    // it would shift every later clip earlier in the downloaded track.
    let duration = calculate_mix_output_duration(
        clip.trim_start,
        clip.duration,
        &clip.segment.trim_segments,
        speed_points,
    );
    if duration <= 0.0001 {
        return Ok(None);
    }
    let path = temp_root.join(format!("{file_stem}_silent.wav"));
    write_silent_wav(&path, duration)?;
    Ok(Some((path, false)))
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
    #[cfg(not(feature = "recorder-worker"))]
    let ffmpeg_component = crate::gui::settings_ui::download_manager::ffmpeg_dependency::acquire_ffmpeg_with_badge_message(
        &audio_ffmpeg_download_message(),
    )?;
    #[cfg(not(feature = "recorder-worker"))]
    let ffmpeg = ffmpeg_component.executable();
    #[cfg(feature = "recorder-worker")]
    let ffmpeg = crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge_message(
        &audio_ffmpeg_download_message(),
    )?;
    let mut command = crate::process_job::HelperCommand::new(&ffmpeg);
    command.args([
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
    ]);
    #[cfg(not(feature = "recorder-worker"))]
    let resources = ffmpeg_component;
    #[cfg(feature = "recorder-worker")]
    let resources = ();
    let output = super::run_ffmpeg(&command, resources)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg MP3 export failed: {stderr}"));
    }
    Ok(())
}

pub fn start_audio_download(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let _active_export_guard = super::ExportActiveGuard::activate()?;

    let started = Instant::now();
    let config: AudioDownloadConfig = parse_json_with_path(args)?;
    config.validate()?;

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
    let temp_root = crate::paths::app_runtime_local_data_dir()
        .join("audio-download")
        .join(timestamp_ms.to_string());
    fs::create_dir_all(&temp_root).map_err(|e| format!("Create audio temp dir: {e}"))?;

    let result = (|| -> Result<serde_json::Value, String> {
        let mut rendered_wavs = Vec::new();
        let mut audible_clip_count = 0usize;
        let mut project_clip_start = 0.0_f64;
        for (index, clip) in config.clips.iter().enumerate() {
            if super::export_user_cancelled() {
                return Ok(json!({ "status": "cancelled" }));
            }
            eprintln!(
                "[AudioDownload] rendering clip {} ({}) for {:?}",
                clip.clip_name, clip.clip_id, config.track_kind
            );
            let sources = build_clip_sources(&config, clip, project_clip_start);
            project_clip_start += clip.duration.max(0.0);
            let speed_points = sorted_speed_points(&clip.segment.speed_points);
            let file_stem = format!("{}_clip_{}", base_name, index);
            let Some((wav_path, audible)) =
                render_clip_audio(&sources, clip, &speed_points, &temp_root, &file_stem)?
            else {
                continue;
            };
            audible_clip_count += usize::from(audible);
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

    if result.is_err() || super::export_user_cancelled() {
        let _ = fs::remove_file(&final_path);
    }
    let _ = fs::remove_dir_all(&temp_root);
    result
}

#[cfg(test)]
#[path = "audio_download_timeline_tests.rs"]
mod timeline_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_clip_audio_download_keeps_source_timeline() {
        let config = parse_json_with_path::<AudioDownloadConfig>(json!({
            "trackKind":"imported", "clips":[{"clipId":"root", "clipName":"Root", "trimStart":7,
                "duration":5, "segment":{"crop":null,"cursorVisibilitySegments":null,
                    "trimSegments":[{"startTime":7,"endTime":12}]} }],
            "audioSegments":[{"rawAudioPath":"audio.wav", "duration":4,"startTime":8,
                "inPoint":0,"outPoint":4}]
        }))
        .unwrap();
        let sources = build_clip_sources(&config, &config.clips[0], 0.0);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].start_offset_sec, 8.0);
        assert_eq!(sources[0].source_out_sec, Some(4.0));
    }

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
