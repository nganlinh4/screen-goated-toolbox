use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::super::audio_time_stretch;
use super::super::mf_audio::MfAudioDecoder;
use super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};

pub const MIX_OUTPUT_SAMPLE_RATE: u32 = 48_000;
pub const MIX_OUTPUT_CHANNELS: u32 = 2;

const MIXER_INTEGRATION_STEP_SEC: f64 = 0.005;
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

fn normalized_trim_segments(
    trim_start: f64,
    duration: f64,
    trim_segments: &[TrimSegment],
) -> Vec<TrimSegment> {
    if trim_segments.is_empty() {
        return vec![TrimSegment {
            start_time: trim_start,
            end_time: trim_start + duration.max(0.0),
        }];
    }
    trim_segments.to_vec()
}

fn get_speed(time: f64, points: &[SpeedPoint]) -> f64 {
    if points.is_empty() {
        return 1.0;
    }

    let idx = points.partition_point(|point| point.time < time);
    if idx == 0 {
        return points[0].speed;
    }
    if idx >= points.len() {
        return points.last().unwrap().speed;
    }

    let left = &points[idx - 1];
    let right = &points[idx];
    let t = (time - left.time) / (right.time - left.time).max(1e-9);
    let cos_t = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
    left.speed + (right.speed - left.speed) * cos_t
}

fn get_audio_volume(time: f64, points: &[DeviceAudioPoint]) -> f64 {
    if points.is_empty() {
        return 1.0;
    }

    let idx = points.partition_point(|point| point.time < time);
    if idx == 0 {
        return points[0].volume.clamp(0.0, 1.0);
    }
    if idx >= points.len() {
        return points.last().unwrap().volume.clamp(0.0, 1.0);
    }

    let left = &points[idx - 1];
    let right = &points[idx];
    let t = (time - left.time) / (right.time - left.time).max(1e-9);
    let cos_t = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
    (left.volume + (right.volume - left.volume) * cos_t).clamp(0.0, 1.0)
}

fn implicit_edge_fade_multiplier(time: f64, start_time: f64, end_time: f64, fade_sec: f64) -> f64 {
    if fade_sec <= 0.0 || end_time <= start_time {
        return 1.0;
    }
    let duration = end_time - start_time;
    let fade = fade_sec.min(duration / 2.0).max(0.0);
    if fade <= 0.0 {
        return 1.0;
    }
    if time <= start_time || time >= end_time {
        return 0.0;
    }
    let fade_in = if time - start_time < fade {
        (1.0 - (((time - start_time) / fade) * std::f64::consts::PI).cos()) / 2.0
    } else {
        1.0
    };
    let fade_out = if end_time - time < fade {
        (1.0 - (((end_time - time) / fade) * std::f64::consts::PI).cos()) / 2.0
    } else {
        1.0
    };
    (fade_in * fade_out).clamp(0.0, 1.0)
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

fn curve_has_audible_points(points: &[DeviceAudioPoint]) -> bool {
    points.iter().any(|point| point.volume > 0.0001)
}

struct OutputTimeMapper {
    trim_segments: Vec<TrimSegment>,
    speed_points: Vec<SpeedPoint>,
    segment_idx: usize,
    cursor_source_time: f64,
    cursor_output_time: f64,
}

impl OutputTimeMapper {
    fn new(trim_segments: Vec<TrimSegment>, speed_points: Vec<SpeedPoint>) -> Self {
        let cursor_source_time = trim_segments
            .first()
            .map(|segment| segment.start_time)
            .unwrap_or(0.0);
        Self {
            trim_segments,
            speed_points,
            segment_idx: 0,
            cursor_source_time,
            cursor_output_time: 0.0,
        }
    }

    fn map_source_time(&mut self, target_time: f64) -> Option<f64> {
        if self.trim_segments.is_empty() {
            return Some(0.0);
        }

        while self.segment_idx < self.trim_segments.len() {
            let segment = &self.trim_segments[self.segment_idx];
            if target_time < segment.start_time {
                return Some(self.cursor_output_time);
            }
            if self.cursor_source_time < segment.start_time {
                self.cursor_source_time = segment.start_time;
            }
            if target_time <= self.cursor_source_time {
                return Some(self.cursor_output_time);
            }
            if target_time <= segment.end_time {
                self.integrate_to(target_time);
                return Some(self.cursor_output_time);
            }
            self.integrate_to(segment.end_time);
            self.segment_idx += 1;
            if self.segment_idx < self.trim_segments.len() {
                self.cursor_source_time = self.trim_segments[self.segment_idx].start_time;
            }
        }

        None
    }

    fn integrate_to(&mut self, target_time: f64) {
        while self.cursor_source_time < target_time - 1e-9 {
            let step_end = (self.cursor_source_time + MIXER_INTEGRATION_STEP_SEC).min(target_time);
            let mid_time = (self.cursor_source_time + step_end) * 0.5;
            let speed = get_speed(mid_time, &self.speed_points).clamp(0.1, 16.0);
            self.cursor_output_time += (step_end - self.cursor_source_time) / speed;
            self.cursor_source_time = step_end;
        }
    }
}

fn convert_f32le_to_i16_bytes(pcm: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((pcm.len() / 4) * 2);
    for chunk in pcm.chunks_exact(4) {
        let sample = f32::from_le_bytes(chunk.try_into().unwrap());
        let pcm_i16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        bytes.extend_from_slice(&pcm_i16.to_le_bytes());
    }
    bytes
}

fn create_wav_writer(
    wav_path: &Path,
) -> Result<hound::WavWriter<std::io::BufWriter<File>>, String> {
    let spec = hound::WavSpec {
        channels: MIX_OUTPUT_CHANNELS as u16,
        sample_rate: MIX_OUTPUT_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    hound::WavWriter::create(wav_path, spec).map_err(|e| format!("Create mixed WAV: {e}"))
}

fn append_silence_frames(
    writer: &mut hound::WavWriter<std::io::BufWriter<File>>,
    frames: u64,
    channels: usize,
) -> Result<(), String> {
    for _ in 0..frames {
        for _ in 0..channels {
            writer
                .write_sample(0i16)
                .map_err(|e| format!("Write silent WAV sample: {e}"))?;
        }
    }
    Ok(())
}

fn append_f32le_chunk_to_wav(
    writer: &mut hound::WavWriter<std::io::BufWriter<File>>,
    pcm: &[u8],
) -> Result<(), String> {
    for sample_bytes in convert_f32le_to_i16_bytes(pcm).chunks_exact(2) {
        let sample = i16::from_le_bytes(sample_bytes.try_into().unwrap());
        writer
            .write_sample(sample)
            .map_err(|e| format!("Write mixed WAV sample: {e}"))?;
    }
    Ok(())
}

fn mix_pcm_chunk_into_raw_file(
    raw_file: &mut File,
    output_start_time: f64,
    pcm: &[u8],
    channels: usize,
) -> Result<(), String> {
    if pcm.is_empty() || channels == 0 {
        return Ok(());
    }

    let mixed_bytes = convert_f32le_to_i16_bytes(pcm);
    if mixed_bytes.is_empty() {
        return Ok(());
    }

    let start_frame = (output_start_time * MIX_OUTPUT_SAMPLE_RATE as f64)
        .round()
        .max(0.0) as u64;
    let byte_offset = start_frame
        .saturating_mul(channels as u64)
        .saturating_mul(2);

    raw_file
        .seek(SeekFrom::Start(byte_offset))
        .map_err(|e| format!("Seek audio mix file: {e}"))?;

    let mut existing = vec![0u8; mixed_bytes.len()];
    let read = raw_file
        .read(&mut existing)
        .map_err(|e| format!("Read audio mix file: {e}"))?;
    if read < existing.len() {
        existing[read..].fill(0);
    }

    let mut combined = Vec::with_capacity(mixed_bytes.len());
    for (new_chunk, existing_chunk) in mixed_bytes.chunks_exact(2).zip(existing.chunks_exact(2)) {
        let new_sample = i16::from_le_bytes(new_chunk.try_into().unwrap()) as i32;
        let existing_sample = i16::from_le_bytes(existing_chunk.try_into().unwrap()) as i32;
        let mixed = (existing_sample + new_sample).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        combined.extend_from_slice(&mixed.to_le_bytes());
    }

    raw_file
        .seek(SeekFrom::Start(byte_offset))
        .map_err(|e| format!("Seek audio mix file for write: {e}"))?;
    raw_file
        .write_all(&combined)
        .map_err(|e| format!("Write audio mix file: {e}"))?;
    Ok(())
}

fn mix_source_into_raw_file(
    raw_file: &mut File,
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

    let channels = decoder.channels() as usize;
    let mut stretcher =
        audio_time_stretch::ExportAudioStretcher::new(MIX_OUTPUT_SAMPLE_RATE, channels);
    let mut last_output_end_time = 0.0f64;
    let mut last_chunk_time = 0.0f64;
    let mut last_source_duration_sec = 0.0f64;

    loop {
        let Some((pcm, ts_100ns)) = decoder.read_samples()? else {
            break;
        };
        let decoded_time = ts_100ns as f64 / 10_000_000.0;
        let channels = decoder.channels() as usize;
        let Some((pcm, decoded_time, source_duration_sec)) = trim_pcm_to_source_window(
            pcm,
            decoded_time,
            channels,
            source.source_in_sec,
            source_out_sec,
        ) else {
            if source_out_sec.is_some_and(|out_sec| decoded_time >= out_sec) {
                break;
            }
            continue;
        };
        let chunk_time =
            (decoded_time / source.playback_rate.max(0.0001)) + source.start_offset_sec;
        let Some(segment) = trim_segments.get(segment_idx) else {
            break;
        };

        if chunk_time > segment.end_time {
            segment_idx += 1;
            if let Some(next_segment) = trim_segments.get(segment_idx) {
                let _ = decoder.seek((next_segment.start_time * 10_000_000.0) as i64);
                continue;
            }
            break;
        }
        if chunk_time < segment.start_time {
            continue;
        }

        let input_frames = pcm.len() / (channels * 4);
        if input_frames == 0 {
            continue;
        }

        let speed = (get_speed(chunk_time, speed_points) * source.playback_rate.max(0.0001))
            .clamp(0.1, 100.0);
        let mut processed = stretcher.stretch(&pcm, speed);
        apply_audio_volume_envelope(
            &mut processed,
            chunk_time,
            source_duration_sec,
            channels,
            &source.volume_points,
            (source.implicit_edge_fade_sec > 0.0).then_some((
                source.start_offset_sec,
                source.start_offset_sec + trim_segments.last().map(|s| s.end_time).unwrap_or(0.0),
                source.implicit_edge_fade_sec,
            )),
        );
        let Some(output_start_time) = mapper.map_source_time(chunk_time) else {
            continue;
        };
        mix_pcm_chunk_into_raw_file(raw_file, output_start_time, &processed, channels)?;
        if !processed.is_empty() {
            last_output_end_time = output_start_time
                + processed.len() as f64 / (channels as f64 * 4.0) / MIX_OUTPUT_SAMPLE_RATE as f64;
            last_chunk_time = chunk_time;
            last_source_duration_sec = source_duration_sec;
        }
    }

    let tail = stretcher.finish();
    if !tail.is_empty() {
        let tail_duration_sec =
            tail.len() as f64 / (channels as f64 * 4.0) / MIX_OUTPUT_SAMPLE_RATE as f64;
        let mut tail = tail;
        apply_audio_volume_envelope(
            &mut tail,
            last_chunk_time + last_source_duration_sec,
            tail_duration_sec,
            channels,
            &source.volume_points,
            (source.implicit_edge_fade_sec > 0.0).then_some((
                source.start_offset_sec,
                source.start_offset_sec + trim_segments.last().map(|s| s.end_time).unwrap_or(0.0),
                source.implicit_edge_fade_sec,
            )),
        );
        mix_pcm_chunk_into_raw_file(raw_file, last_output_end_time, &tail, channels)?;
    }

    Ok(())
}

fn write_wav_from_raw_pcm(raw_path: &Path, wav_path: &Path) -> Result<(), String> {
    let mut writer = create_wav_writer(wav_path)?;
    let mut raw_file = File::open(raw_path).map_err(|e| format!("Open mixed PCM: {e}"))?;
    let mut buffer = vec![0u8; 32 * 1024];

    loop {
        let read = raw_file
            .read(&mut buffer)
            .map_err(|e| format!("Read mixed PCM: {e}"))?;
        if read == 0 {
            break;
        }
        for sample_bytes in buffer[..read].chunks_exact(2) {
            let sample = i16::from_le_bytes(sample_bytes.try_into().unwrap());
            writer
                .write_sample(sample)
                .map_err(|e| format!("Write mixed WAV sample: {e}"))?;
        }
    }

    writer
        .finalize()
        .map_err(|e| format!("Finalize mixed WAV: {e}"))?;
    Ok(())
}

fn build_single_source_preprocessed_wav(
    source: &ExportAudioSource,
    speed_points: &[SpeedPoint],
    trim_segments: &[TrimSegment],
    wav_path: &Path,
) -> Result<Option<PathBuf>, String> {
    eprintln!("[Export][AudioPrep] native MF/Rust path");

    let decoder = MfAudioDecoder::new_with_output_format(
        &source.path,
        Some(MIX_OUTPUT_SAMPLE_RATE),
        Some(MIX_OUTPUT_CHANNELS),
    )?;
    if trim_segments.is_empty() {
        return Ok(None);
    }

    let mut writer = create_wav_writer(wav_path)?;
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

    let channels = decoder.channels() as usize;
    let mut stretcher =
        audio_time_stretch::ExportAudioStretcher::new(MIX_OUTPUT_SAMPLE_RATE, channels);
    let mut written_frames = 0u64;
    let mut last_chunk_time = 0.0f64;
    let mut last_source_duration_sec = 0.0f64;

    loop {
        let Some((pcm, ts_100ns)) = decoder.read_samples()? else {
            break;
        };
        let decoded_time = ts_100ns as f64 / 10_000_000.0;
        let channels = decoder.channels() as usize;
        let Some((pcm, decoded_time, source_duration_sec)) = trim_pcm_to_source_window(
            pcm,
            decoded_time,
            channels,
            source.source_in_sec,
            source_out_sec,
        ) else {
            if source_out_sec.is_some_and(|out_sec| decoded_time >= out_sec) {
                break;
            }
            continue;
        };
        let chunk_time =
            (decoded_time / source.playback_rate.max(0.0001)) + source.start_offset_sec;
        let Some(segment) = trim_segments.get(segment_idx) else {
            break;
        };

        if chunk_time > segment.end_time {
            segment_idx += 1;
            if let Some(next_segment) = trim_segments.get(segment_idx) {
                let _ = decoder.seek((next_segment.start_time * 10_000_000.0) as i64);
                continue;
            }
            break;
        }
        if chunk_time < segment.start_time {
            continue;
        }

        let input_frames = pcm.len() / (channels * 4);
        if input_frames == 0 {
            continue;
        }

        let speed = (get_speed(chunk_time, speed_points) * source.playback_rate.max(0.0001))
            .clamp(0.1, 100.0);
        let mut processed = stretcher.stretch(&pcm, speed);
        apply_audio_volume_envelope(
            &mut processed,
            chunk_time,
            source_duration_sec,
            channels,
            &source.volume_points,
            (source.implicit_edge_fade_sec > 0.0).then_some((
                source.start_offset_sec,
                source.start_offset_sec + trim_segments.last().map(|s| s.end_time).unwrap_or(0.0),
                source.implicit_edge_fade_sec,
            )),
        );
        let Some(output_start_time) = mapper.map_source_time(chunk_time) else {
            continue;
        };
        let target_start_frame = (output_start_time * MIX_OUTPUT_SAMPLE_RATE as f64)
            .round()
            .max(0.0) as u64;
        if target_start_frame > written_frames {
            append_silence_frames(&mut writer, target_start_frame - written_frames, channels)?;
            written_frames = target_start_frame;
        }
        append_f32le_chunk_to_wav(&mut writer, &processed)?;
        written_frames += processed.len() as u64 / (channels as u64 * 4);
        last_chunk_time = chunk_time;
        last_source_duration_sec = source_duration_sec;
    }

    let mut tail = stretcher.finish();
    if !tail.is_empty() {
        let tail_duration_sec =
            tail.len() as f64 / (channels as f64 * 4.0) / MIX_OUTPUT_SAMPLE_RATE as f64;
        apply_audio_volume_envelope(
            &mut tail,
            last_chunk_time + last_source_duration_sec,
            tail_duration_sec,
            channels,
            &source.volume_points,
            (source.implicit_edge_fade_sec > 0.0).then_some((
                source.start_offset_sec,
                source.start_offset_sec + trim_segments.last().map(|s| s.end_time).unwrap_or(0.0),
                source.implicit_edge_fade_sec,
            )),
        );
        append_f32le_chunk_to_wav(&mut writer, &tail)?;
        written_frames += tail.len() as u64 / (channels as u64 * 4);
    }

    writer
        .finalize()
        .map_err(|e| format!("Finalize mixed WAV: {e}"))?;
    if written_frames == 0 {
        let _ = fs::remove_file(wav_path);
        return Ok(None);
    }
    Ok(Some(wav_path.to_path_buf()))
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
    if active_sources.len() == 1 {
        let t0 = Instant::now();
        return build_single_source_preprocessed_wav(
            &active_sources[0],
            speed_points,
            &trim_segments,
            &wav_path,
        )
        .inspect(|path| {
            eprintln!(
                "[Export][AudioPrep] single-source mix: {:.3}s (output={})",
                t0.elapsed().as_secs_f64(),
                path.is_some()
            );
        });
    }

    let raw_path = temp_dir.join(format!("{file_stem}_audio_mix.pcm"));
    let mut raw_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&raw_path)
        .map_err(|e| format!("Create audio mix file: {e}"))?;

    let result = (|| -> Result<Option<PathBuf>, String> {
        for source in &active_sources {
            if !Path::new(&source.path).exists() {
                continue;
            }
            let t0 = Instant::now();
            mix_source_into_raw_file(&mut raw_file, source, &trim_segments, speed_points)?;
            eprintln!(
                "[Export][AudioPrep] mixed source '{}' in {:.3}s",
                Path::new(&source.path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<audio>"),
                t0.elapsed().as_secs_f64()
            );
        }

        let t0 = Instant::now();
        let mixed_len = raw_file
            .metadata()
            .map_err(|e| format!("Inspect mixed PCM: {e}"))?
            .len();
        if mixed_len == 0 {
            return Ok(None);
        }

        write_wav_from_raw_pcm(&raw_path, &wav_path)?;
        eprintln!(
            "[Export][AudioPrep] write mixed wav: {:.3}s",
            t0.elapsed().as_secs_f64()
        );
        Ok(Some(wav_path.clone()))
    })();

    let _ = fs::remove_file(&raw_path);
    if result.is_err() {
        let _ = fs::remove_file(&wav_path);
    }
    result
}
