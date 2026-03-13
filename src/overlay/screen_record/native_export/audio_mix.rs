use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};
use super::super::mf_audio::MfAudioDecoder;

pub const MIX_OUTPUT_SAMPLE_RATE: u32 = 48_000;
pub const MIX_OUTPUT_CHANNELS: u32 = 2;

const MIXER_INTEGRATION_STEP_SEC: f64 = 0.005;

#[derive(Debug, Clone)]
pub struct ExportAudioSource {
    pub path: String,
    pub volume_points: Vec<DeviceAudioPoint>,
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

fn resample_pcm_bytes(input: &[u8], speed: f64, channels: usize) -> Vec<u8> {
    if (speed - 1.0).abs() < 0.001 || input.is_empty() || channels == 0 {
        return input.to_vec();
    }
    if !input.len().is_multiple_of(4) {
        return input.to_vec();
    }

    let samples = input.len() / 4;
    if samples < channels * 2 {
        return input.to_vec();
    }

    let mut input_f32 = vec![0.0f32; samples];
    unsafe {
        std::ptr::copy_nonoverlapping(
            input.as_ptr(),
            input_f32.as_mut_ptr() as *mut u8,
            input.len(),
        );
    }

    let in_frames = input_f32.len() / channels;
    if in_frames < 2 {
        return input.to_vec();
    }
    let out_frames = ((in_frames as f64) / speed).max(1.0) as usize;
    let mut output_f32 = Vec::with_capacity(out_frames * channels);

    for frame_idx in 0..out_frames {
        let src_idx = frame_idx as f64 * speed;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(in_frames - 1);
        let frac = (src_idx - idx0 as f64) as f32;
        for channel_idx in 0..channels {
            let v0 = input_f32[idx0 * channels + channel_idx];
            let v1 = input_f32[idx1 * channels + channel_idx];
            output_f32.push(v0 + (v1 - v0) * frac);
        }
    }

    let out_bytes = output_f32.len() * 4;
    let mut output_u8 = vec![0u8; out_bytes];
    unsafe {
        std::ptr::copy_nonoverlapping(
            output_f32.as_ptr() as *const u8,
            output_u8.as_mut_ptr(),
            out_bytes,
        );
    }
    output_u8
}

fn apply_audio_volume_envelope(
    pcm: &mut [u8],
    source_start_time: f64,
    source_duration_sec: f64,
    channels: usize,
    points: &[DeviceAudioPoint],
) {
    if pcm.is_empty() || channels == 0 {
        return;
    }

    let frames = pcm.len() / (channels * 4);
    if frames == 0 {
        return;
    }

    if points
        .iter()
        .all(|point| (point.volume.clamp(0.0, 1.0) - 1.0).abs() < 0.0001)
    {
        return;
    }

    let frame_time_step = if source_duration_sec <= 0.0 {
        0.0
    } else {
        source_duration_sec / frames as f64
    };

    for frame_idx in 0..frames {
        let sample_time = source_start_time + ((frame_idx as f64) + 0.5) * frame_time_step;
        let volume = get_audio_volume(sample_time, points) as f32;
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for channel_idx in 0..channels {
            let sample_idx = ((frame_idx * channels) + channel_idx) * 4;
            let sample = f32::from_le_bytes(
                pcm[sample_idx..sample_idx + 4]
                    .try_into()
                    .unwrap(),
            );
            pcm[sample_idx..sample_idx + 4]
                .copy_from_slice(&(sample * volume).clamp(-1.0, 1.0).to_le_bytes());
        }
    }
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
        let cursor_source_time = trim_segments.first().map(|segment| segment.start_time).unwrap_or(0.0);
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
            let step_end =
                (self.cursor_source_time + MIXER_INTEGRATION_STEP_SEC).min(target_time);
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

    let start_frame = (output_start_time * MIX_OUTPUT_SAMPLE_RATE as f64).round().max(0.0) as u64;
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
    for (new_chunk, existing_chunk) in mixed_bytes
        .chunks_exact(2)
        .zip(existing.chunks_exact(2))
    {
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
    if trim_segments[0].start_time > 0.0 {
        let _ = decoder.seek((trim_segments[0].start_time * 10_000_000.0) as i64);
    }

    loop {
        let Some((pcm, ts_100ns)) = decoder.read_samples()? else {
            break;
        };
        let chunk_time = ts_100ns as f64 / 10_000_000.0;
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

        let channels = decoder.channels() as usize;
        let input_frames = if channels == 0 {
            0
        } else {
            pcm.len() / (channels * 4)
        };
        if input_frames == 0 {
            continue;
        }

        let source_duration_sec = input_frames as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
        let speed = get_speed(chunk_time, speed_points).clamp(0.1, 16.0);
        let mut processed = resample_pcm_bytes(&pcm, speed, channels);
        apply_audio_volume_envelope(
            &mut processed,
            chunk_time,
            source_duration_sec,
            channels,
            &source.volume_points,
        );
        let Some(output_start_time) = mapper.map_source_time(chunk_time) else {
            continue;
        };
        mix_pcm_chunk_into_raw_file(raw_file, output_start_time, &processed, channels)?;
    }

    Ok(())
}

fn write_wav_from_raw_pcm(raw_path: &Path, wav_path: &Path) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: MIX_OUTPUT_CHANNELS as u16,
        sample_rate: MIX_OUTPUT_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec)
        .map_err(|e| format!("Create mixed WAV: {e}"))?;
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
        .filter(|source| !source.path.trim().is_empty() && curve_has_audible_points(&source.volume_points))
        .cloned()
        .collect();
    if active_sources.is_empty() {
        return Ok(None);
    }

    fs::create_dir_all(temp_dir).map_err(|e| format!("Create audio mix temp dir: {e}"))?;
    let raw_path = temp_dir.join(format!("{file_stem}_audio_mix.pcm"));
    let wav_path = temp_dir.join(format!("{file_stem}_audio_mix.wav"));
    let trim_segments = normalized_trim_segments(trim_start, duration, trim_segments);
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
            mix_source_into_raw_file(&mut raw_file, source, &trim_segments, speed_points)?;
        }

        let mixed_len = raw_file
            .metadata()
            .map_err(|e| format!("Inspect mixed PCM: {e}"))?
            .len();
        if mixed_len == 0 {
            return Ok(None);
        }

        write_wav_from_raw_pcm(&raw_path, &wav_path)?;
        Ok(Some(wav_path.clone()))
    })();

    let _ = fs::remove_file(&raw_path);
    if result.is_err() {
        let _ = fs::remove_file(&wav_path);
    }
    result
}
