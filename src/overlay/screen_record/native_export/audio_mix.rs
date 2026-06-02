mod mix_buffer;
mod time_map;
mod wav_fast;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use super::super::mf_audio::MfAudioDecoder;
use super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};

use self::mix_buffer::FloatMixBuffer;
pub use self::time_map::calculate_mix_output_duration;
use self::time_map::{
    OutputTimeMapper, average_output_tempo, curve_has_audible_points, get_audio_volume, get_speed,
    implicit_edge_fade_multiplier, normalized_trim_segments, source_project_end_time,
};
use self::wav_fast::{DecodedAudioChunk, fast_retime_f32le, read_wav_fast_chunks};

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

fn apply_audio_volume_envelope(
    pcm: &mut [u8],
    source_start_time: f64,
    source_duration_sec: f64,
    channels: usize,
    points: &[DeviceAudioPoint],
    implicit_fade: Option<(f64, f64, f64)>,
) {
    if pcm.is_empty() || channels == 0 {
        return;
    }

    let frames = pcm.len() / (channels * 4);
    if frames == 0 {
        return;
    }

    let has_editable_volume = points
        .iter()
        .any(|point| (point.volume.clamp(0.0, 1.0) - 1.0).abs() >= 0.0001);
    if !has_editable_volume && implicit_fade.is_none() {
        return;
    }

    let frame_time_step = if source_duration_sec <= 0.0 {
        0.0
    } else {
        source_duration_sec / frames as f64
    };

    for frame_idx in 0..frames {
        let sample_time = source_start_time + ((frame_idx as f64) + 0.5) * frame_time_step;
        let fade = implicit_fade
            .map(|(start, end, fade_sec)| {
                implicit_edge_fade_multiplier(sample_time, start, end, fade_sec)
            })
            .unwrap_or(1.0);
        let volume = (get_audio_volume(sample_time, points) * fade) as f32;
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for channel_idx in 0..channels {
            let sample_idx = ((frame_idx * channels) + channel_idx) * 4;
            let sample = f32::from_le_bytes(pcm[sample_idx..sample_idx + 4].try_into().unwrap());
            pcm[sample_idx..sample_idx + 4]
                .copy_from_slice(&(sample * volume).clamp(-1.0, 1.0).to_le_bytes());
        }
    }
}

fn trim_pcm_to_source_window(
    pcm: Vec<u8>,
    decoded_time: f64,
    channels: usize,
    source_in_sec: Option<f64>,
    source_out_sec: Option<f64>,
) -> Option<(Vec<u8>, f64, f64)> {
    if channels == 0 {
        return None;
    }
    let bytes_per_frame = channels * 4;
    let frames = pcm.len() / bytes_per_frame;
    if frames == 0 {
        return None;
    }
    let duration_sec = frames as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
    let chunk_start = decoded_time;
    let chunk_end = decoded_time + duration_sec;
    let window_start = source_in_sec
        .filter(|value| value.is_finite())
        .unwrap_or(0.0);
    let window_end = source_out_sec
        .filter(|value| value.is_finite())
        .unwrap_or(f64::INFINITY);
    let keep_start = chunk_start.max(window_start);
    let keep_end = chunk_end.min(window_end);
    if keep_end <= keep_start {
        return None;
    }
    let start_frame = ((keep_start - chunk_start) * MIX_OUTPUT_SAMPLE_RATE as f64)
        .floor()
        .clamp(0.0, frames as f64) as usize;
    let end_frame = ((keep_end - chunk_start) * MIX_OUTPUT_SAMPLE_RATE as f64)
        .ceil()
        .clamp(start_frame as f64, frames as f64) as usize;
    if end_frame <= start_frame {
        return None;
    }
    let start_byte = start_frame * bytes_per_frame;
    let end_byte = end_frame * bytes_per_frame;
    let next_pcm = pcm[start_byte..end_byte].to_vec();
    let next_time = chunk_start + start_frame as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
    let next_duration = (end_frame - start_frame) as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
    Some((next_pcm, next_time, next_duration))
}

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

fn audio_ffmpeg_download_message() -> String {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
        .screen_record_audio_ffmpeg_downloading
        .to_string()
}

struct AudioRetimeContext<'a> {
    trim_segments: &'a [TrimSegment],
    speed_points: &'a [SpeedPoint],
    temp_dir: &'a Path,
    file_stem: &'a str,
    source_index: usize,
    fallback_duration: f64,
    ffmpeg_path_cache: &'a mut Option<PathBuf>,
}

fn render_pitch_preserved_source_with_ffmpeg(
    source: &ExportAudioSource,
    context: AudioRetimeContext<'_>,
) -> Result<Option<ExportAudioSource>, String> {
    let AudioRetimeContext {
        trim_segments,
        speed_points,
        temp_dir,
        file_stem,
        source_index,
        fallback_duration,
        ffmpeg_path_cache,
    } = context;
    let Some((tempo, output_start)) =
        average_output_tempo(source, trim_segments, speed_points, fallback_duration)
    else {
        return Ok(None);
    };
    let effective_tempo = tempo * source.playback_rate.max(0.0001);
    if (effective_tempo - 1.0).abs() <= 0.0001 {
        return Ok(None);
    }

    let ffmpeg = match ffmpeg_path_cache {
        Some(path) => path.clone(),
        None => {
            let path = crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge_message(
                &audio_ffmpeg_download_message(),
            )?;
            *ffmpeg_path_cache = Some(path.clone());
            path
        }
    };
    fs::create_dir_all(temp_dir).map_err(|e| format!("Create audio retime temp dir: {e}"))?;
    let out_path = temp_dir.join(format!("{file_stem}_audio_retime_{source_index}.wav"));
    let source_in = source
        .source_in_sec
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let source_out = source.source_out_sec.filter(|value| value.is_finite());
    let mut atrim = format!("atrim=start={source_in:.6}");
    if let Some(out) = source_out
        && out > source_in
    {
        atrim.push_str(&format!(":end={out:.6}"));
    }
    let filter = format!(
        "{atrim},asetpts=PTS-STARTPTS,{},aresample={},aformat=sample_fmts=s16:channel_layouts=stereo",
        atempo_chain(effective_tempo),
        MIX_OUTPUT_SAMPLE_RATE
    );
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
            &filter,
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
        effective_tempo,
        started_at.elapsed().as_secs_f64()
    );
    Ok(Some(ExportAudioSource {
        path: out_path.to_string_lossy().to_string(),
        volume_points: source.volume_points.clone(),
        start_offset_sec: output_start,
        source_in_sec: None,
        source_out_sec: None,
        playback_rate: 1.0,
        implicit_edge_fade_sec: source.implicit_edge_fade_sec,
    }))
}

struct DecodedChunkContext<'a> {
    source: &'a ExportAudioSource,
    trim_segments: &'a [TrimSegment],
    speed_points: &'a [SpeedPoint],
    mapper: &'a mut OutputTimeMapper,
    segment_idx: &'a mut usize,
    source_out_sec: Option<f64>,
}

fn process_decoded_chunk(
    mixer: &mut FloatMixBuffer,
    chunk: DecodedAudioChunk,
    context: DecodedChunkContext<'_>,
) -> Result<bool, String> {
    let DecodedChunkContext {
        source,
        trim_segments,
        speed_points,
        mapper,
        segment_idx,
        source_out_sec,
    } = context;
    let Some((pcm, decoded_time, source_duration_sec)) = trim_pcm_to_source_window(
        chunk.pcm,
        chunk.decoded_time,
        chunk.channels,
        source.source_in_sec,
        source_out_sec,
    ) else {
        return Ok(!source_out_sec.is_some_and(|out_sec| chunk.decoded_time >= out_sec));
    };
    let chunk_time = (decoded_time / source.playback_rate.max(0.0001)) + source.start_offset_sec;
    let Some(segment) = trim_segments.get(*segment_idx) else {
        return Ok(false);
    };

    if chunk_time > segment.end_time {
        *segment_idx += 1;
        return Ok(*segment_idx < trim_segments.len());
    }
    if chunk_time < segment.start_time {
        return Ok(true);
    }

    let input_frames = pcm.len() / (chunk.channels * 4);
    if input_frames == 0 {
        return Ok(true);
    }

    let speed =
        (get_speed(chunk_time, speed_points) * source.playback_rate.max(0.0001)).clamp(0.1, 100.0);
    let mut processed = fast_retime_f32le(&pcm, chunk.channels, speed);
    apply_audio_volume_envelope(
        &mut processed,
        chunk_time,
        source_duration_sec,
        chunk.channels,
        &source.volume_points,
        (source.implicit_edge_fade_sec > 0.0).then_some((
            source.start_offset_sec,
            source.start_offset_sec + trim_segments.last().map(|s| s.end_time).unwrap_or(0.0),
            source.implicit_edge_fade_sec,
        )),
    );
    let Some(output_start_time) = mapper.map_source_time(chunk_time) else {
        return Ok(true);
    };
    mixer.mix_f32le(output_start_time, &processed, chunk.channels)?;
    Ok(true)
}

fn mix_mf_source_into_buffer(
    mixer: &mut FloatMixBuffer,
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
) -> Result<(), String> {
    let decoder = MfAudioDecoder::new_with_output_format(
        &source.path,
        Some(MIX_OUTPUT_SAMPLE_RATE),
        Some(MIX_OUTPUT_CHANNELS),
    )?;
    if trim_segments.is_empty() {
        return Ok(());
    }

    let mut mapper = OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec());
    let mut segment_idx = 0usize;
    let initial_seek_sec = match source.source_in_sec {
        Some(in_sec) if in_sec > 0.0 => in_sec,
        _ => trim_segments[0].start_time.max(0.0),
    };
    if initial_seek_sec > 0.0 {
        let _ = decoder.seek((initial_seek_sec * 10_000_000.0) as i64);
    }
    let source_out_sec = source.source_out_sec.filter(|out| out.is_finite());

    loop {
        let Some((pcm, ts_100ns)) = decoder.read_samples()? else {
            break;
        };
        let chunk = DecodedAudioChunk {
            pcm,
            decoded_time: ts_100ns as f64 / 10_000_000.0,
            channels: decoder.channels() as usize,
        };
        if !process_decoded_chunk(
            mixer,
            chunk,
            DecodedChunkContext {
                source,
                trim_segments,
                speed_points,
                mapper: &mut mapper,
                segment_idx: &mut segment_idx,
                source_out_sec,
            },
        )? {
            if let Some(next_segment) = trim_segments.get(segment_idx) {
                let _ = decoder.seek((next_segment.start_time * 10_000_000.0) as i64);
                continue;
            } else {
                break;
            }
        }
    }

    Ok(())
}

fn mix_wav_fast_source_into_buffer(
    mixer: &mut FloatMixBuffer,
    source: &ExportAudioSource,
    speed_points: &[SpeedPoint],
    trim_segments: &[TrimSegment],
) -> Result<bool, String> {
    let Some(mut chunks) = read_wav_fast_chunks(&source.path)? else {
        return Ok(false);
    };
    let mut mapper = OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec());
    let mut segment_idx = 0usize;
    let source_out_sec = source.source_out_sec.filter(|out| out.is_finite());

    while let Some(chunk) = chunks.pop_front() {
        if !process_decoded_chunk(
            mixer,
            chunk,
            DecodedChunkContext {
                source,
                trim_segments,
                speed_points,
                mapper: &mut mapper,
                segment_idx: &mut segment_idx,
                source_out_sec,
            },
        )? {
            break;
        }
    }

    Ok(true)
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
    if let Some(retimed_source) = render_pitch_preserved_source_with_ffmpeg(
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
            end_time: output_duration
                .max(source_project_end_time(&retimed_source, fallback_duration)),
        }];
        if mix_wav_fast_source_into_buffer(mixer, &retimed_source, &[], &identity_segments)? {
            let _ = fs::remove_file(&retimed_source.path);
            return Ok("ffmpeg_atempo");
        }
        let _ = fs::remove_file(&retimed_source.path);
    }
    if mix_wav_fast_source_into_buffer(mixer, source, speed_points, trim_segments)? {
        Ok("wav_fast_path")
    } else {
        mix_mf_source_into_buffer(mixer, source, trim_segments, speed_points)?;
        Ok("mf_or_symphonia")
    }
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
        let mut wav_fast_sources = 0usize;
        let mut ffmpeg_atempo_sources = 0usize;
        let mut fallback_sources = 0usize;
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
            if path_kind == "wav_fast_path" {
                wav_fast_sources += 1;
            } else if path_kind == "ffmpeg_atempo" {
                ffmpeg_atempo_sources += 1;
            } else {
                fallback_sources += 1;
            }
            let elapsed = t0.elapsed().as_secs_f64();
            if elapsed > 1.0 || path_kind != "wav_fast_path" {
                eprintln!(
                    "[Export][AudioPrep] mixed source '{}' via {} in {:.3}s",
                    Path::new(&source.path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("<audio>"),
                    path_kind,
                    elapsed
                );
            }
        }

        if !mixer.has_audio() {
            return Ok(None);
        }
        eprintln!(
            "[Export][AudioPrep] mixed {} sources in {:.3}s (wav_fast={}, ffmpeg_atempo={}, fallback={})",
            active_sources.len(),
            t_mix.elapsed().as_secs_f64(),
            wav_fast_sources,
            ffmpeg_atempo_sources,
            fallback_sources
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
