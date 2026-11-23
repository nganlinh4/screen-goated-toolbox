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
        let offset = if apply_offset { source_offset_sec } else { 0.0 };
        let (source_start, source_end, frame_count) =
            subtitle_audio_span(trim_segment, offset, sample_rate)?;
        let segment_start = pcm_samples.len();
        let segment_end = segment_start
            .checked_add(frame_count)
            .filter(|end| *end <= (u32::MAX / 2) as usize)
            .ok_or("Subtitle audio exceeds the WAV size limit")?;
        pcm_samples
            .try_reserve(frame_count)
            .map_err(|_| "Could not allocate subtitle audio".to_string())?;
        pcm_samples.resize(segment_end, 0);

        if frame_count == 0 || source_end <= 0.0 {
            continue;
        }
        decoder.seek((source_start.max(0.0) * MF_100NS_PER_SEC) as i64)?;

        while let Some((bytes, timestamp_100ns)) = decoder.read_samples()? {
            let timestamp_sec = timestamp_100ns as f64 / MF_100NS_PER_SEC;
            let total_float_samples = bytes.len() / 4;
            if total_float_samples == 0 {
                continue;
            }
            let chunk_frames = total_float_samples / channels;
            let chunk_duration_sec = chunk_frames as f64 / sample_rate;
            let chunk_end_sec = timestamp_sec + chunk_duration_sec;
            if chunk_end_sec <= source_start {
                continue;
            }
            if timestamp_sec >= source_end {
                break;
            }
            place_audio_chunk(
                &mut pcm_samples[segment_start..segment_end],
                &bytes,
                channels,
                sample_rate,
                source_start,
                timestamp_sec,
            );
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

fn subtitle_audio_span(
    trim_segment: &SubtitleTrimSegment,
    offset: f64,
    sample_rate: f64,
) -> Result<(f64, f64, usize), String> {
    let start = trim_segment.start_time;
    let end = trim_segment.end_time;
    if !start.is_finite() || !end.is_finite() || !offset.is_finite() || end < start {
        return Err("Invalid subtitle audio trim or offset".to_string());
    }
    let frame_count = ((end - start) * sample_rate).round();
    if frame_count > (u32::MAX / 2) as f64 {
        return Err("Subtitle audio exceeds the WAV size limit".to_string());
    }
    // The timeline starts the audio at `offset`, matching preview and export.
    Ok((start - offset, end - offset, frame_count as usize))
}

fn place_audio_chunk(
    output: &mut [i16],
    bytes: &[u8],
    channels: usize,
    sample_rate: f64,
    source_start: f64,
    timestamp_sec: f64,
) {
    let chunk_start = ((timestamp_sec - source_start) * sample_rate).round() as i64;
    let frame_size = channels * 4;
    for (index, frame) in bytes.chunks_exact(frame_size).enumerate() {
        let output_frame = chunk_start.saturating_add(index as i64);
        if output_frame < 0 {
            continue;
        }
        let Some(destination) = output.get_mut(output_frame as usize) else {
            break;
        };
        let sample = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
        *destination = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
    }
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

#[cfg(test)]
mod audio_timing_tests {
    use super::{SubtitleTrimSegment, build_trimmed_wav, place_audio_chunk, subtitle_audio_span};

    fn float_bytes(samples: &[f32]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect()
    }

    #[test]
    fn delayed_microphone_keeps_leading_silence_and_compact_duration() {
        let trim = SubtitleTrimSegment {
            start_time: 0.0,
            end_time: 1.0,
        };
        let (source_start, source_end, frames) = subtitle_audio_span(&trim, 0.2, 10.0).unwrap();
        assert_eq!((source_start, source_end, frames), (-0.2, 0.8, 10));
        let mut compact = vec![0; frames];
        place_audio_chunk(
            &mut compact,
            &float_bytes(&[0.5; 8]),
            1,
            10.0,
            source_start,
            0.0,
        );
        assert_eq!(&compact[..2], &[0, 0]);
        assert!(compact[2..].iter().all(|sample| *sample == 16383));
    }

    #[test]
    fn early_microphone_skips_source_start_and_keeps_missing_tail_silent() {
        let trim = SubtitleTrimSegment {
            start_time: 0.0,
            end_time: 1.0,
        };
        let (source_start, source_end, frames) = subtitle_audio_span(&trim, -0.2, 10.0).unwrap();
        assert_eq!((source_start, source_end, frames), (0.2, 1.2, 10));
        let mut compact = vec![0; frames];
        let samples = [0.0, 0.0, 0.25, 0.25, 0.25, 0.25, 0.25, 0.25, 0.25, 0.25];
        place_audio_chunk(
            &mut compact,
            &float_bytes(&samples),
            1,
            10.0,
            source_start,
            0.0,
        );
        assert!(compact[..8].iter().all(|sample| *sample == 8191));
        assert_eq!(&compact[8..], &[0, 0]);
    }

    #[test]
    fn disjoint_trims_keep_independent_timeline_positions_and_gaps() {
        let trims = [
            SubtitleTrimSegment {
                start_time: 0.0,
                end_time: 0.5,
            },
            SubtitleTrimSegment {
                start_time: 1.0,
                end_time: 1.5,
            },
        ];
        let mut compact = Vec::new();
        for trim in &trims {
            let (source_start, _, frames) = subtitle_audio_span(trim, 0.2, 10.0).unwrap();
            let mut segment = vec![0; frames];
            place_audio_chunk(
                &mut segment,
                &float_bytes(&[0.5; 3]),
                1,
                10.0,
                source_start,
                0.0,
            );
            place_audio_chunk(
                &mut segment,
                &float_bytes(&[0.25; 2]),
                1,
                10.0,
                source_start,
                0.9,
            );
            compact.extend(segment);
        }
        assert_eq!(compact, [0, 0, 16383, 16383, 16383, 0, 8191, 8191, 0, 0]);
    }

    #[test]
    fn nonfinite_and_reversed_timing_is_rejected() {
        let trim = SubtitleTrimSegment {
            start_time: 1.0,
            end_time: 0.0,
        };
        assert!(subtitle_audio_span(&trim, 0.0, 16_000.0).is_err());
        let trim = SubtitleTrimSegment {
            start_time: 0.0,
            end_time: 1.0,
        };
        assert!(subtitle_audio_span(&trim, f64::NAN, 16_000.0).is_err());
    }

    #[test]
    fn decoded_wav_keeps_delay_and_duration_on_the_compact_timeline() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sgt-subtitle-audio-test-{}-{}.wav",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        struct TestFile(std::path::PathBuf);
        impl Drop for TestFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _file = TestFile(path.clone());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..16_000 {
            let sample = if frame < 4_000 { 8_000i16 } else { 16_000i16 };
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        let trim = [SubtitleTrimSegment {
            start_time: 0.0,
            end_time: 1.0,
        }];
        let wav = build_trimmed_wav(path.to_str().unwrap(), &trim, 0.25, true).unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 16_000);
        assert_eq!(samples[2_000], 0);
        assert!((samples[6_000] - 8_000).abs() < 5);
        assert!((samples[10_000] - 16_000).abs() < 5);
    }
}
