use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::overlay::screen_record::owned_file::OwnedFile;

use super::super::audio_mix::{
    ExportAudioSource, IMPLICIT_AUDIO_EDGE_FADE_SEC, build_preprocessed_audio_mix,
};
use super::super::config::{self, ExportConfig, ImportedAudioSegmentConfig};

pub(super) type AudioVideoPrep = (
    String,
    Option<String>,
    Option<OwnedFile>,
    bool,
    Vec<config::DeviceAudioPoint>,
    Vec<config::SpeedPoint>,
);

/// The project's own source is the only valid input: falling back to another
/// capture would render unrelated footage with this project's edits.
fn resolve_source_video_path(configured: &str) -> Result<String, String> {
    let path = configured.trim();
    if path.is_empty() {
        return Err("Export has no source video path".to_string());
    }
    if !Path::new(path).is_file() {
        return Err(format!("Source video not found at {path}"));
    }
    Ok(path.to_string())
}

/// Intermediate audio belongs to app scratch space, never the user's output folder.
fn export_audio_temp_dir() -> PathBuf {
    crate::paths::app_temp_dir().join("export-audio")
}

pub(super) fn prepare_audio_and_video(config: &ExportConfig) -> Result<AudioVideoPrep, String> {
    let source_video_path = resolve_source_video_path(&config.source_video_path)?;

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
    let device_offset_requires_mix = requires_device_offset_mix(
        &config.device_audio_path,
        has_audible_device_audio,
        config.segment.device_audio_offset_sec,
    );
    let t_audio_start = Instant::now();
    let use_preprocessed_audio = config.format != "gif"
        && (speed_changes_audio_timeline
            || device_offset_requires_mix
            || (!config.mic_audio_path.trim().is_empty() && has_audible_mic_audio)
            || has_audio_segments
            || has_narration_segments);
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let mixed_audio = if use_preprocessed_audio {
        build_mixed_audio(MixedAudioBuild {
            config,
            device_audio_points: &device_audio_points,
            mic_audio_points: &mic_audio_points,
            speed_points: &speed_points,
            has_audible_device_audio,
            has_audible_mic_audio,
            temp_dir: &export_audio_temp_dir(),
            timestamp_ms,
        })?
        .map(OwnedFile::new)
    } else {
        None
    };
    let source_audio_path = if let Some(mix) = &mixed_audio {
        Some(mix.path().to_string_lossy().to_string())
    } else if !config.device_audio_path.is_empty()
        && has_audible_device_audio
        && !device_offset_requires_mix
        && config.format != "gif"
    {
        Some(config.device_audio_path.clone())
    } else {
        None
    };
    let audio_is_preprocessed = mixed_audio.is_some();
    eprintln!(
        "[Export][Timing] Audio preprocessing: {:.3}s (preprocessed={})",
        t_audio_start.elapsed().as_secs_f64(),
        audio_is_preprocessed
    );

    Ok((
        source_video_path,
        source_audio_path,
        mixed_audio,
        audio_is_preprocessed,
        device_audio_points,
        speed_points,
    ))
}

fn requires_device_offset_mix(path: &str, audible: bool, offset: f64) -> bool {
    !path.trim().is_empty() && audible && offset != 0.0
}

struct MixedAudioBuild<'a> {
    config: &'a ExportConfig,
    device_audio_points: &'a [config::DeviceAudioPoint],
    mic_audio_points: &'a [config::DeviceAudioPoint],
    speed_points: &'a [config::SpeedPoint],
    has_audible_device_audio: bool,
    has_audible_mic_audio: bool,
    temp_dir: &'a Path,
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
        ctx.temp_dir,
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

#[cfg(test)]
mod tests {
    use super::{prepare_audio_and_video, requires_device_offset_mix, resolve_source_video_path};
    use crate::overlay::screen_record::native_export::config::ExportConfig;
    use std::path::PathBuf;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let mut random = [0u8; 8];
            getrandom::fill(&mut random).unwrap();
            let path = std::env::temp_dir().join(format!(
                "sgt-export-audio-test-{}-{}",
                std::process::id(),
                u64::from_ne_bytes(random)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Ok(entries) = std::fs::read_dir(&self.0) {
                for entry in entries.flatten() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn missing_project_source_fails_instead_of_using_another_capture() {
        let dir = TestDirectory::new();
        let moved = dir.0.join("moved.mp4");
        let error = resolve_source_video_path(&moved.to_string_lossy()).unwrap_err();
        assert!(error.contains("Source video not found"), "{error}");
        assert!(resolve_source_video_path("  ").is_err());
        std::fs::write(&moved, []).unwrap();
        assert_eq!(
            resolve_source_video_path(&moved.to_string_lossy()).unwrap(),
            moved.to_string_lossy()
        );
    }

    #[test]
    fn audible_device_delays_use_the_preprocessed_timeline() {
        assert!(requires_device_offset_mix("device.wav", true, 0.3));
        assert!(requires_device_offset_mix("device.wav", true, -0.3));
        assert!(!requires_device_offset_mix("device.wav", true, 0.0));
        assert!(!requires_device_offset_mix("device.wav", false, 0.3));
        assert!(!requires_device_offset_mix("  ", true, 0.3));
    }

    #[test]
    fn device_delay_and_cuts_apply_to_the_exported_audio() {
        let dir = TestDirectory::new();
        let video = dir.0.join("video.mp4");
        let audio = dir.0.join("device.wav");
        std::fs::write(&video, []).unwrap();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let write_source = |amplitude| {
            let mut writer = hound::WavWriter::create(&audio, spec).unwrap();
            for _ in 0..(48_000 * 2 * 2) {
                writer.write_sample(amplitude).unwrap();
            }
            writer.finalize().unwrap();
        };
        write_source(16_000i16);
        let config: ExportConfig = serde_json::from_value(serde_json::json!({
            "width": 100, "height": 100, "framerate": 30,
            "sourceVideoPath": video, "deviceAudioPath": audio,
            "outputDir": dir.0, "trimStart": 0.0, "duration": 1.7,
            "segment": {
                "crop": null, "cursorVisibilitySegments": null,
                "deviceAudioOffsetSec": 0.5,
                "trimSegments": [
                    {"startTime": 0.0, "endTime": 0.7},
                    {"startTime": 1.0, "endTime": 1.7}
                ]
            },
            "backgroundConfig": {
                "scale": 100.0, "borderRadius": 0.0, "backgroundType": "solid",
                "shadow": 0.0, "cursorScale": 1.0
            }
        }))
        .unwrap();
        let (_, path, cleanup, preprocessed, _, _) = prepare_audio_and_video(&config).unwrap();
        assert!(preprocessed);
        let path = path.expect("offset audio must be preprocessed");
        assert_ne!(path, config.device_audio_path);
        let mut reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 48_000 * 2 * 14 / 10);
        assert_eq!(samples[48_000 * 2 / 10], 0); // 0.1 seconds, before the source starts
        assert!(samples[48_000 * 2 * 6 / 10].abs() > 100); // 0.6 seconds
        assert!(samples[48_000 * 2 * 9 / 10].abs() > 100); // second retained region
        drop(reader);
        assert!(!path.starts_with(dir.0.to_string_lossy().as_ref()));
        drop(cleanup);
        assert!(!std::path::Path::new(&path).exists());

        write_source(0i16);
        let (_, source_audio, _, preprocessed, _, _) = prepare_audio_and_video(&config).unwrap();
        assert!(!preprocessed);
        assert!(
            source_audio.is_none(),
            "silent offset mix must not fall back to raw audio"
        );
    }
}
