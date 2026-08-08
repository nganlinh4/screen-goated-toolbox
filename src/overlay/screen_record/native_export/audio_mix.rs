mod ffmpeg_retime;
mod mix_buffer;
mod retime_plan;
mod stretch_mix;
mod time_map;
mod wav_fast;
mod wsola;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};

use self::ffmpeg_retime::{AudioRetimeContext, render_pitch_preserved_source_with_ffmpeg};
use self::mix_buffer::FloatMixBuffer;
use self::stretch_mix::mix_source_with_stretch;
pub use self::time_map::calculate_mix_output_duration;
use self::time_map::{curve_has_audible_points, normalized_trim_segments};

pub const MIX_OUTPUT_SAMPLE_RATE: u32 = 48_000;
pub const MIX_OUTPUT_CHANNELS: u32 = 2;

pub const IMPLICIT_AUDIO_EDGE_FADE_SEC: f64 = 0.12;

#[derive(Debug, Clone)]
pub struct ExportAudioSource {
    pub path: String,
    pub volume_points: Vec<DeviceAudioPoint>,
    /// Where on the project timeline this source begins playing.
    pub start_offset_sec: f64,
    /// Optional source-internal trim — read from the source starting at
    /// `source_in_sec` (default 0) and stop at `source_out_sec` (default end).
    pub source_in_sec: Option<f64>,
    pub source_out_sec: Option<f64>,
    /// Per-source playback rate (1.0 = original). Values >1 play faster and
    /// shrink the timeline footprint; <1 plays slower and stretches it.
    pub playback_rate: f64,
    pub implicit_edge_fade_sec: f64,
}

struct MixSourceContext<'a> {
    trim_segments: &'a [TrimSegment],
    speed_points: &'a [SpeedPoint],
    temp_dir: &'a Path,
    file_stem: &'a str,
    source_index: usize,
    fallback_duration: f64,
    output_duration: f64,
    ffmpeg_path_cache: &'a mut Option<PathBuf>,
}

/// Both paths preserve pitch. FFmpeg's `atempo` is preferred where it is exactly
/// correct — a single constant tempo over an uncut span — because it is a mature,
/// well-tuned implementation. Everything else (ramps, stepped speeds, cuts inside a
/// source) goes to the WSOLA stretcher, which follows the curve hop by hop.
fn mix_source_into_buffer(
    mixer: &mut FloatMixBuffer,
    source: &ExportAudioSource,
    context: MixSourceContext<'_>,
) -> Result<&'static str, String> {
    let MixSourceContext {
        trim_segments,
        speed_points,
        temp_dir,
        file_stem,
        source_index,
        fallback_duration,
        output_duration,
        ffmpeg_path_cache,
    } = context;
    // The retimed file already carries the speed curve and the trim cuts, so it is
    // mixed straight onto the output timeline with no speed points of its own.
    if let Some(retimed) = render_pitch_preserved_source_with_ffmpeg(
        source,
        AudioRetimeContext {
            trim_segments,
            speed_points,
            temp_dir,
            file_stem,
            source_index,
            fallback_duration,
            ffmpeg_path_cache,
        },
    )? {
        let identity_segments = vec![TrimSegment {
            start_time: 0.0,
            end_time: output_duration.max(retimed.output_end),
        }];
        let result = mix_source_with_stretch(
            mixer,
            &retimed.source,
            &identity_segments,
            &[],
            retimed.output_end,
        );
        let _ = fs::remove_file(&retimed.source.path);
        result?;
        return Ok("ffmpeg_atempo");
    }
    mix_source_with_stretch(
        mixer,
        source,
        trim_segments,
        speed_points,
        fallback_duration,
    )
}

pub fn build_preprocessed_audio_mix(
    sources: &[ExportAudioSource],
    speed_points: &[SpeedPoint],
    trim_start: f64,
    duration: f64,
    trim_segments: &[TrimSegment],
    temp_dir: &Path,
    file_stem: &str,
) -> Result<Option<PathBuf>, String> {
    let active_sources: Vec<ExportAudioSource> = sources
        .iter()
        .filter(|source| {
            !source.path.trim().is_empty() && curve_has_audible_points(&source.volume_points)
        })
        .cloned()
        .collect();
    if active_sources.is_empty() {
        return Ok(None);
    }

    fs::create_dir_all(temp_dir).map_err(|e| format!("Create audio mix temp dir: {e}"))?;
    let wav_path = temp_dir.join(format!("{file_stem}_audio_mix.wav"));
    let trim_segments = normalized_trim_segments(trim_start, duration, trim_segments);
    let output_duration =
        calculate_mix_output_duration(trim_start, duration, &trim_segments, speed_points);
    let mut mixer = FloatMixBuffer::new(MIX_OUTPUT_CHANNELS as usize, output_duration);
    let result = (|| -> Result<Option<PathBuf>, String> {
        let mut ffmpeg_path_cache = None;
        let t_mix = Instant::now();
        for (source_index, source) in active_sources.iter().enumerate() {
            if !Path::new(&source.path).exists() {
                continue;
            }
            let t0 = Instant::now();
            let path_kind = mix_source_into_buffer(
                &mut mixer,
                source,
                MixSourceContext {
                    trim_segments: &trim_segments,
                    speed_points,
                    temp_dir,
                    file_stem,
                    source_index,
                    fallback_duration: duration,
                    output_duration,
                    ffmpeg_path_cache: &mut ffmpeg_path_cache,
                },
            )?;
            // log_info! rather than eprintln! so the retime path a given export took
            // survives in session.log; a GUI launch has no console to read.
            crate::log_info!(
                "[Export][AudioPrep] mixed source '{}' via {} in {:.3}s",
                Path::new(&source.path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<audio>"),
                path_kind,
                t0.elapsed().as_secs_f64()
            );
        }

        if !mixer.has_audio() {
            // Returning None here makes the caller fall back to the raw source audio,
            // which carries no speed map at all. That fallback is right for a
            // genuinely silent project and wrong for a mixer bug, and the two are
            // indistinguishable downstream — so say which happened.
            crate::log_info!(
                "[Export][AudioPrep] mix came out silent for {} source(s); export will fall back to raw audio (no speed map)",
                active_sources.len()
            );
            return Ok(None);
        }
        eprintln!(
            "[Export][AudioPrep] mixed {} sources in {:.3}s",
            active_sources.len(),
            t_mix.elapsed().as_secs_f64()
        );
        let t0 = Instant::now();
        mixer.write_wav(&wav_path)?;
        eprintln!(
            "[Export][AudioPrep] write mixed wav: {:.3}s",
            t0.elapsed().as_secs_f64()
        );
        Ok(Some(wav_path.clone()))
    })();

    if result.is_err() {
        let _ = fs::remove_file(&wav_path);
    }
    result
}
