use std::io::Cursor;

use crate::overlay::screen_record::mf_audio::MfAudioDecoder;

use super::types::SubtitleTrimSegment;

pub const MIN_SUBTITLE_DURATION_SEC: f64 = 0.1;

const MF_100NS_PER_SEC: f64 = 10_000_000.0;

pub fn build_trimmed_wav(
    source_path: &str,
    trim_segments: &[SubtitleTrimSegment],
    source_offset_sec: f64,
    apply_offset: bool,
) -> Result<Vec<u8>, String> {
    let decoder = MfAudioDecoder::new_with_output_format(source_path, Some(16_000), Some(1))?;
    let sample_rate = decoder.sample_rate() as f64;
    let channels = decoder.channels().max(1) as usize;
    let mut pcm_samples: Vec<i16> = Vec::new();

    for trim_segment in trim_segments {
        let adjusted_start =
            (trim_segment.start_time + if apply_offset { source_offset_sec } else { 0.0 }).max(0.0);
        let adjusted_end = (trim_segment.end_time
            + if apply_offset { source_offset_sec } else { 0.0 })
        .max(adjusted_start);
        decoder.seek((adjusted_start * MF_100NS_PER_SEC) as i64)?;

        while let Some((bytes, timestamp_100ns)) = decoder.read_samples()? {
            let timestamp_sec = timestamp_100ns as f64 / MF_100NS_PER_SEC;
            let total_float_samples = bytes.len() / 4;
            if total_float_samples == 0 {
                continue;
            }
            let frame_count = total_float_samples / channels;
            let chunk_duration_sec = frame_count as f64 / sample_rate;
            let chunk_end_sec = timestamp_sec + chunk_duration_sec;
            if chunk_end_sec <= adjusted_start {
                continue;
            }
            if timestamp_sec >= adjusted_end {
                break;
            }

            let overlap_start = adjusted_start.max(timestamp_sec);
            let overlap_end = adjusted_end.min(chunk_end_sec);
            if overlap_end <= overlap_start {
                continue;
            }

            let start_frame = ((overlap_start - timestamp_sec) * sample_rate)
                .floor()
                .max(0.0) as usize;
            let end_frame = ((overlap_end - timestamp_sec) * sample_rate)
                .ceil()
                .max(start_frame as f64) as usize;
            let floats = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect::<Vec<_>>();

            for frame_index in start_frame..end_frame.min(frame_count) {
                let sample = floats[frame_index * channels];
                let clamped = sample.clamp(-1.0, 1.0);
                pcm_samples.push((clamped * i16::MAX as f32) as i16);
            }
        }
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        hound::WavWriter::new(&mut cursor, spec).map_err(|e| format!("Create WAV writer: {e}"))?;
    for sample in pcm_samples {
        writer
            .write_sample(sample)
            .map_err(|e| format!("Write WAV sample: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Finalize WAV: {e}"))?;
    Ok(cursor.into_inner())
}

/// Window duration used when scanning for low-energy regions during VAD-aware splitting.
const VAD_WINDOW_MS: u32 = 25;

/// Compute per-window RMS over a mono view of the input samples.
/// Returns `(rms_per_window, frames_per_window)`.
fn compute_window_rms(samples: &[i16], channels: usize, sample_rate: u32) -> (Vec<f32>, usize) {
    let channels = channels.max(1);
    let window_frames = ((sample_rate as f64 * VAD_WINDOW_MS as f64) / 1000.0).max(1.0) as usize;
    let total_frames = samples.len() / channels;
    if total_frames == 0 || window_frames == 0 {
        return (Vec::new(), window_frames);
    }
    let window_count = total_frames.div_ceil(window_frames);
    let mut rms = Vec::with_capacity(window_count);
    for window_index in 0..window_count {
        let start_frame = window_index * window_frames;
        let end_frame = (start_frame + window_frames).min(total_frames);
        let mut sum_sq: f64 = 0.0;
        let mut count: usize = 0;
        for frame in start_frame..end_frame {
            let mono = samples[frame * channels] as f64 / i16::MAX as f64;
            sum_sq += mono * mono;
            count += 1;
        }
        let value = if count > 0 {
            (sum_sq / count as f64).sqrt() as f32
        } else {
            0.0
        };
        rms.push(value);
    }
    (rms, window_frames)
}

/// Snap each ideal split frame to the quietest window within `search_radius_sec`.
/// Returned positions are strictly increasing and bounded by `(0, total_frames)`.
pub fn snap_split_frames_to_silence(
    samples: &[i16],
    channels: usize,
    sample_rate: u32,
    ideal_frame_positions: &[usize],
    search_radius_sec: f64,
) -> Vec<usize> {
    if ideal_frame_positions.is_empty() {
        return Vec::new();
    }
    let channels = channels.max(1);
    let total_frames = samples.len() / channels;
    if total_frames < 2 {
        return ideal_frame_positions.to_vec();
    }
    let (window_rms, window_frames) = compute_window_rms(samples, channels, sample_rate);
    if window_rms.is_empty() || window_frames == 0 {
        return ideal_frame_positions.to_vec();
    }
    let radius_windows = ((search_radius_sec * sample_rate as f64) / window_frames as f64)
        .ceil()
        .max(1.0) as usize;
    let last_window = window_rms.len() - 1;
    let mut snapped = Vec::with_capacity(ideal_frame_positions.len());
    let mut min_window_floor: usize = 0;
    for ideal_frame in ideal_frame_positions {
        let clamped_frame = (*ideal_frame).min(total_frames - 1);
        let ideal_window = (clamped_frame / window_frames).min(last_window);
        let lo = ideal_window
            .saturating_sub(radius_windows)
            .max(min_window_floor);
        let hi = (ideal_window + radius_windows).min(last_window);
        if hi <= lo {
            snapped.push(clamped_frame);
            min_window_floor = (ideal_window + 1).min(last_window);
            continue;
        }
        let mut best_index = ideal_window;
        let mut best_rms = f32::INFINITY;
        let mut best_distance = usize::MAX;
        for (window_index, rms_value) in window_rms.iter().enumerate().take(hi + 1).skip(lo) {
            let rms_value = *rms_value;
            let distance = window_index.abs_diff(ideal_window);
            let is_better = rms_value < best_rms - 1e-6
                || (rms_value <= best_rms + 1e-6 && distance < best_distance);
            if is_better {
                best_rms = rms_value;
                best_distance = distance;
                best_index = window_index;
            }
        }
        let snapped_frame = (best_index * window_frames + window_frames / 2).min(total_frames - 1);
        snapped.push(snapped_frame);
        min_window_floor = (best_index + 1).min(last_window);
    }

    // Final guard: keep positions strictly increasing within (0, total_frames).
    let mut prev: usize = 0;
    for position in snapped.iter_mut() {
        if *position <= prev {
            *position = prev + 1;
        }
        if *position >= total_frames {
            *position = total_frames - 1;
        }
        prev = *position;
    }
    snapped
}

/// Build half-open `[start_frame, end_frame)` chunk ranges for VAD-aware splitting.
/// Initial split positions are evenly spaced; each is then snapped to the quietest
/// window within `search_radius_sec` so chunks end on natural silence whenever possible.
pub fn build_silence_aware_split_frames(
    samples: &[i16],
    channels: usize,
    sample_rate: u32,
    split_parts: usize,
    search_radius_sec: f64,
) -> Vec<(usize, usize)> {
    let channels = channels.max(1);
    let total_frames = samples.len() / channels;
    if split_parts <= 1 || total_frames == 0 {
        return vec![(0, total_frames)];
    }
    let split_parts = split_parts.min(total_frames.max(1));
    let ideal_positions: Vec<usize> = (1..split_parts)
        .map(|i| total_frames * i / split_parts)
        .collect();
    let snapped = snap_split_frames_to_silence(
        samples,
        channels,
        sample_rate,
        &ideal_positions,
        search_radius_sec,
    );
    let mut ranges = Vec::with_capacity(split_parts);
    let mut start: usize = 0;
    for boundary in snapped {
        if boundary > start {
            ranges.push((start, boundary));
            start = boundary;
        }
    }
    if start < total_frames {
        ranges.push((start, total_frames));
    }
    ranges
}

pub fn compact_to_source_time(
    compact_time: f64,
    trim_segments: &[SubtitleTrimSegment],
    source_duration: f64,
) -> f64 {
    let mut remaining = compact_time.max(0.0);
    for segment in trim_segments {
        let len = (segment.end_time - segment.start_time).max(0.0);
        if remaining <= len {
            return (segment.start_time + remaining).clamp(0.0, source_duration);
        }
        remaining -= len;
    }
    trim_segments
        .last()
        .map(|segment| segment.end_time)
        .unwrap_or(source_duration)
        .clamp(0.0, source_duration)
}
