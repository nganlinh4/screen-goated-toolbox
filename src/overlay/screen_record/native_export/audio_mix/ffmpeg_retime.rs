//! Pitch-preserving retime of one audio source onto the output timeline.
//!
//! Only runs when [`retime_plan`] proves a single constant tempo reproduces the
//! timeline exactly. Everything else falls through to the WSOLA stretcher.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use super::super::config::{SpeedPoint, TrimSegment};
use super::retime_plan::{ConstantTempoRetime, build_constant_tempo_retime};
use super::{ExportAudioSource, MIX_OUTPUT_SAMPLE_RATE};

pub(super) struct RetimedSource {
    pub(super) source: ExportAudioSource,
    /// Where the retimed audio ends on the output timeline.
    pub(super) output_end: f64,
}

pub(super) struct AudioRetimeContext<'a> {
    pub(super) trim_segments: &'a [TrimSegment],
    pub(super) speed_points: &'a [SpeedPoint],
    pub(super) temp_dir: &'a Path,
    pub(super) file_stem: &'a str,
    pub(super) source_index: usize,
    pub(super) fallback_duration: f64,
    pub(super) ffmpeg_path_cache: &'a mut Option<PathBuf>,
}

/// `atempo` only accepts 0.5..=2.0 per instance, so larger factors chain.
fn atempo_chain(tempo: f64) -> String {
    let mut remaining = tempo.clamp(0.05, 64.0);
    let mut filters = Vec::new();
    while remaining > 2.0 {
        let factor = remaining.sqrt().min(2.0);
        filters.push(format!("atempo={factor:.6}"));
        remaining /= factor;
    }
    while remaining < 0.5 {
        filters.push("atempo=0.500000".to_string());
        remaining /= 0.5;
    }
    filters.push(format!("atempo={remaining:.6}"));
    filters.join(",")
}

/// The result is padded then hard-trimmed to the planned length so `atempo`'s own
/// rounding cannot leave the tail short and slide later sources out of place.
fn build_audio_filter(retime: &ConstantTempoRetime) -> String {
    format!(
        "atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS,{},aresample={MIX_OUTPUT_SAMPLE_RATE},apad,atrim=duration={:.6},asetpts=PTS-STARTPTS,aformat=sample_fmts=s16:channel_layouts=stereo",
        retime.source_start,
        retime.source_end,
        atempo_chain(retime.tempo),
        retime.output_duration,
    )
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

#[cfg(feature = "recorder-worker")]
fn resolve_ffmpeg(cache: &mut Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = cache {
        return Ok(path.clone());
    }
    let path = crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge_message(
        &audio_ffmpeg_download_message(),
    )?;
    *cache = Some(path.clone());
    Ok(path)
}

/// Returns `None` when the source cannot be retimed by a single tempo; the caller
/// then falls through to the WSOLA stretcher.
pub(super) fn render_pitch_preserved_source_with_ffmpeg(
    source: &ExportAudioSource,
    context: AudioRetimeContext<'_>,
) -> Result<Option<RetimedSource>, String> {
    let AudioRetimeContext {
        trim_segments,
        speed_points,
        temp_dir,
        file_stem,
        source_index,
        fallback_duration,
        ffmpeg_path_cache,
    } = context;
    let Some(retime) =
        build_constant_tempo_retime(source, trim_segments, speed_points, fallback_duration)
    else {
        return Ok(None);
    };

    #[cfg(not(feature = "recorder-worker"))]
    let ffmpeg_component = crate::gui::settings_ui::download_manager::ffmpeg_dependency::acquire_ffmpeg_with_badge_message(
        &audio_ffmpeg_download_message(),
    )?;
    #[cfg(not(feature = "recorder-worker"))]
    let ffmpeg = {
        let path = ffmpeg_component.executable();
        *ffmpeg_path_cache = Some(path.clone());
        path
    };
    #[cfg(feature = "recorder-worker")]
    let ffmpeg = resolve_ffmpeg(ffmpeg_path_cache)?;
    fs::create_dir_all(temp_dir).map_err(|e| format!("Create audio retime temp dir: {e}"))?;
    let out_path = temp_dir.join(format!("{file_stem}_audio_retime_{source_index}.wav"));

    let started_at = Instant::now();
    let output = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            &source.path,
            "-vn",
            "-af",
            &build_audio_filter(&retime),
            out_path.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|err| format!("Failed to launch FFmpeg audio retime: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&out_path);
        return Err(format!("FFmpeg audio retime failed: {stderr}"));
    }
    eprintln!(
        "[Export][AudioPrep] ffmpeg atempo source '{}' tempo={:.3} in {:.3}s",
        Path::new(&source.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<audio>"),
        retime.tempo,
        started_at.elapsed().as_secs_f64()
    );

    Ok(Some(RetimedSource {
        source: ExportAudioSource {
            path: out_path.to_string_lossy().to_string(),
            volume_points: retime.volume_points,
            start_offset_sec: retime.output_start,
            source_in_sec: None,
            source_out_sec: Some(retime.output_duration),
            playback_rate: 1.0,
            implicit_edge_fade_sec: source.implicit_edge_fade_sec,
        },
        output_end: retime.output_start + retime.output_duration,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tempo_product(chain: &str) -> f64 {
        chain
            .split(',')
            .map(|filter| {
                filter
                    .trim_start_matches("atempo=")
                    .parse::<f64>()
                    .expect("atempo factor")
            })
            .product()
    }

    #[test]
    fn atempo_chain_multiplies_out_to_the_requested_tempo() {
        for tempo in [0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 9.0, 16.0] {
            let chain = atempo_chain(tempo);
            let product = parse_tempo_product(&chain);
            assert!(
                (product - tempo).abs() < 1e-3,
                "tempo {tempo} -> {chain} = {product}"
            );
            for filter in chain.split(',') {
                let factor: f64 = filter.trim_start_matches("atempo=").parse().unwrap();
                assert!(
                    (0.5..=2.0).contains(&factor),
                    "atempo factor {factor} outside FFmpeg's range"
                );
            }
        }
    }

    #[test]
    fn audio_filter_trims_the_window_and_pins_the_output_length() {
        let filter = build_audio_filter(&ConstantTempoRetime {
            source_start: 4.0,
            source_end: 12.0,
            tempo: 2.0,
            output_start: 1.0,
            output_duration: 4.0,
            volume_points: Vec::new(),
        });

        assert!(filter.starts_with("atrim=start=4.000000:end=12.000000,"));
        assert!(filter.contains("atempo=2.000000"));
        assert!(filter.contains("apad,atrim=duration=4.000000"));
        assert!(filter.ends_with("aformat=sample_fmts=s16:channel_layouts=stereo"));
    }
}
