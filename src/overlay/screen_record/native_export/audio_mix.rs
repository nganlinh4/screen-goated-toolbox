use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use super::super::mf_audio::MfAudioDecoder;
use super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};

pub const MIX_OUTPUT_SAMPLE_RATE: u32 = 48_000;
pub const MIX_OUTPUT_CHANNELS: u32 = 2;

const MIXER_INTEGRATION_STEP_SEC: f64 = 0.005;
pub const IMPLICIT_AUDIO_EDGE_FADE_SEC: f64 = 0.12;
const WAV_FAST_CHUNK_FRAMES: usize = 8192;

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
    if points.is_empty() {
        return true;
    }
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

struct FloatMixBuffer {
    samples: Vec<f32>,
    channels: usize,
}

impl FloatMixBuffer {
    fn new(channels: usize, duration_sec: f64) -> Self {
        let frames = (duration_sec.max(0.0) * MIX_OUTPUT_SAMPLE_RATE as f64).ceil() as usize;
        Self {
            samples: vec![0.0; frames.saturating_mul(channels)],
            channels,
        }
    }

    fn mix_f32le(
        &mut self,
        output_start_time: f64,
        pcm: &[u8],
        channels: usize,
    ) -> Result<(), String> {
        if pcm.is_empty() || channels == 0 {
            return Ok(());
        }
        if channels != self.channels {
            return Err(format!(
                "Audio mix channel mismatch: source={channels}, output={}",
                self.channels
            ));
        }
        let start_frame = (output_start_time * MIX_OUTPUT_SAMPLE_RATE as f64)
            .round()
            .max(0.0) as usize;
        let start_sample = start_frame.saturating_mul(self.channels);
        let source_samples = pcm.len() / 4;
        let required = start_sample.saturating_add(source_samples);
        if required > self.samples.len() {
            self.samples.resize(required, 0.0);
        }
        for (index, chunk) in pcm.chunks_exact(4).enumerate() {
            let sample = f32::from_le_bytes(chunk.try_into().unwrap());
            self.samples[start_sample + index] += sample;
        }
        Ok(())
    }

    fn has_audio(&self) -> bool {
        self.samples.iter().any(|sample| sample.abs() > 0.000_001)
    }

    fn write_wav(&self, wav_path: &Path) -> Result<(), String> {
        let mut writer = create_wav_writer(wav_path)?;
        for sample in &self.samples {
            let pcm_i16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer
                .write_sample(pcm_i16)
                .map_err(|e| format!("Write mixed WAV sample: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("Finalize mixed WAV: {e}"))?;
        Ok(())
    }
}

struct DecodedAudioChunk {
    pcm: Vec<u8>,
    decoded_time: f64,
    channels: usize,
}

fn f32_samples_to_le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn push_wav_frame(
    frames: &mut Vec<[f32; 2]>,
    pending: &mut Option<f32>,
    sample: f32,
    channels: u16,
) {
    match channels {
        1 => frames.push([sample, sample]),
        2 => {
            if let Some(left) = pending.take() {
                frames.push([left, sample]);
            } else {
                *pending = Some(sample);
            }
        }
        _ => {}
    }
}

fn wav_frames_to_output_chunks(
    source_frames: &[[f32; 2]],
    source_sample_rate: u32,
) -> VecDeque<DecodedAudioChunk> {
    let mut chunks = VecDeque::new();
    if source_frames.is_empty() || source_sample_rate == 0 {
        return chunks;
    }

    let output_frames = ((source_frames.len() as u64 * MIX_OUTPUT_SAMPLE_RATE as u64)
        .saturating_add(source_sample_rate as u64 - 1)
        / source_sample_rate as u64) as usize;
    let mut chunk_samples =
        Vec::with_capacity(WAV_FAST_CHUNK_FRAMES * MIX_OUTPUT_CHANNELS as usize);
    let mut chunk_start_frame = 0usize;

    let flush_chunk =
        |samples: &mut Vec<f32>, start_frame: usize, chunks: &mut VecDeque<DecodedAudioChunk>| {
            if samples.is_empty() {
                return;
            }
            chunks.push_back(DecodedAudioChunk {
                pcm: f32_samples_to_le_bytes(samples),
                decoded_time: start_frame as f64 / MIX_OUTPUT_SAMPLE_RATE as f64,
                channels: MIX_OUTPUT_CHANNELS as usize,
            });
            samples.clear();
        };

    for output_frame_idx in 0..output_frames {
        let source_pos =
            output_frame_idx as f64 * source_sample_rate as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
        let left_idx = source_pos.floor() as usize;
        let right_idx = (left_idx + 1).min(source_frames.len() - 1);
        let t = (source_pos - left_idx as f64) as f32;
        let left = source_frames[left_idx];
        let right = source_frames[right_idx];
        chunk_samples.push(left[0] + (right[0] - left[0]) * t);
        chunk_samples.push(left[1] + (right[1] - left[1]) * t);

        if (output_frame_idx + 1).is_multiple_of(WAV_FAST_CHUNK_FRAMES) {
            flush_chunk(&mut chunk_samples, chunk_start_frame, &mut chunks);
            chunk_start_frame = output_frame_idx + 1;
        }
    }
    flush_chunk(&mut chunk_samples, chunk_start_frame, &mut chunks);
    chunks
}

fn read_wav_fast_chunks(path: &str) -> Result<Option<VecDeque<DecodedAudioChunk>>, String> {
    if !path.to_ascii_lowercase().ends_with(".wav") {
        return Ok(None);
    }
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("Open WAV fast path: {e}"))?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || !(spec.channels == 1 || spec.channels == 2) {
        return Ok(None);
    }

    let estimated_frames = reader.duration() as usize / spec.channels as usize;
    let mut source_frames = Vec::with_capacity(estimated_frames);
    let mut pending_stereo_sample = None;

    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            for sample in reader.samples::<f32>() {
                let sample = sample.map_err(|e| format!("Read WAV float sample: {e}"))?;
                push_wav_frame(
                    &mut source_frames,
                    &mut pending_stereo_sample,
                    sample.clamp(-1.0, 1.0),
                    spec.channels,
                );
            }
        }
        (hound::SampleFormat::Int, 16) => {
            for sample in reader.samples::<i16>() {
                let sample = sample.map_err(|e| format!("Read WAV i16 sample: {e}"))?;
                push_wav_frame(
                    &mut source_frames,
                    &mut pending_stereo_sample,
                    sample as f32 / 32768.0,
                    spec.channels,
                );
            }
        }
        (hound::SampleFormat::Int, 24 | 32) => {
            let denom = if spec.bits_per_sample == 24 {
                8_388_608.0
            } else {
                2_147_483_648.0
            };
            for sample in reader.samples::<i32>() {
                let sample = sample.map_err(|e| format!("Read WAV i32 sample: {e}"))?;
                push_wav_frame(
                    &mut source_frames,
                    &mut pending_stereo_sample,
                    (sample as f32 / denom).clamp(-1.0, 1.0),
                    spec.channels,
                );
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(wav_frames_to_output_chunks(
        &source_frames,
        spec.sample_rate,
    )))
}

fn fast_retime_f32le(pcm: &[u8], channels: usize, speed: f64) -> Vec<u8> {
    if pcm.is_empty() || channels == 0 {
        return Vec::new();
    }
    let input_frames = pcm.len() / (channels * 4);
    if input_frames == 0 {
        return Vec::new();
    }
    let speed = speed.clamp(0.05, 64.0);
    if (speed - 1.0).abs() <= 0.0001 {
        return pcm.to_vec();
    }

    let output_frames = ((input_frames as f64) / speed).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(output_frames * channels * 4);
    for output_frame_idx in 0..output_frames {
        let source_pos = output_frame_idx as f64 * speed;
        let left_frame = source_pos.floor().min((input_frames - 1) as f64) as usize;
        let right_frame = (left_frame + 1).min(input_frames - 1);
        let t = (source_pos - left_frame as f64) as f32;
        for channel_idx in 0..channels {
            let left_sample_idx = ((left_frame * channels) + channel_idx) * 4;
            let right_sample_idx = ((right_frame * channels) + channel_idx) * 4;
            let left = f32::from_le_bytes(
                pcm[left_sample_idx..left_sample_idx + 4]
                    .try_into()
                    .unwrap(),
            );
            let right = f32::from_le_bytes(
                pcm[right_sample_idx..right_sample_idx + 4]
                    .try_into()
                    .unwrap(),
            );
            out.extend_from_slice(&(left + (right - left) * t).to_le_bytes());
        }
    }
    out
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

fn source_project_start_time(source: &ExportAudioSource) -> f64 {
    source.start_offset_sec
        + source
            .source_in_sec
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
            / source.playback_rate.max(0.0001)
}

fn source_project_end_time(source: &ExportAudioSource, fallback_duration: f64) -> f64 {
    let source_out = source
        .source_out_sec
        .filter(|value| value.is_finite())
        .unwrap_or(fallback_duration.max(0.0));
    source.start_offset_sec + source_out / source.playback_rate.max(0.0001)
}

fn output_time_for_project_time(
    project_time: f64,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
) -> Option<f64> {
    OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec())
        .map_source_time(project_time)
}

fn average_output_tempo(
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    fallback_duration: f64,
) -> Option<(f64, f64)> {
    let project_start = source_project_start_time(source);
    let project_end = source_project_end_time(source, fallback_duration);
    if project_end <= project_start {
        return None;
    }
    let output_start = output_time_for_project_time(project_start, trim_segments, speed_points)?;
    let output_end = output_time_for_project_time(project_end, trim_segments, speed_points)?;
    if output_end <= output_start {
        return None;
    }
    Some((
        (project_end - project_start) / (output_end - output_start),
        output_start,
    ))
}

fn render_pitch_preserved_source_with_ffmpeg(
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    temp_dir: &Path,
    file_stem: &str,
    source_index: usize,
    fallback_duration: f64,
    ffmpeg_path_cache: &mut Option<PathBuf>,
) -> Result<Option<ExportAudioSource>, String> {
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
    if let Some(out) = source_out {
        if out > source_in {
            atrim.push_str(&format!(":end={out:.6}"));
        }
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

fn process_decoded_chunk(
    mixer: &mut FloatMixBuffer,
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    mapper: &mut OutputTimeMapper,
    segment_idx: &mut usize,
    chunk: DecodedAudioChunk,
    source_out_sec: Option<f64>,
) -> Result<bool, String> {
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
            source,
            trim_segments,
            speed_points,
            &mut mapper,
            &mut segment_idx,
            chunk,
            source_out_sec,
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
            source,
            trim_segments,
            speed_points,
            &mut mapper,
            &mut segment_idx,
            chunk,
            source_out_sec,
        )? {
            break;
        }
    }

    Ok(true)
}

fn mix_source_into_buffer(
    mixer: &mut FloatMixBuffer,
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    temp_dir: &Path,
    file_stem: &str,
    source_index: usize,
    fallback_duration: f64,
    ffmpeg_path_cache: &mut Option<PathBuf>,
) -> Result<&'static str, String> {
    if let Some(retimed_source) = render_pitch_preserved_source_with_ffmpeg(
        source,
        trim_segments,
        speed_points,
        temp_dir,
        file_stem,
        source_index,
        fallback_duration,
        ffmpeg_path_cache,
    )? {
        let identity_segments = vec![TrimSegment {
            start_time: 0.0,
            end_time: fallback_duration
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
    let mut mixer = FloatMixBuffer::new(MIX_OUTPUT_CHANNELS as usize, duration);
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
                &trim_segments,
                speed_points,
                temp_dir,
                file_stem,
                source_index,
                duration,
                &mut ffmpeg_path_cache,
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
