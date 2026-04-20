use std::io::Cursor;

const SAMPLE_RATE: usize = 16_000;
const WINDOW_SAMPLES: usize = 320;
const SPEECH_RMS_THRESHOLD: f32 = 0.015;
const SPEECH_START_WINDOWS: usize = 4;
const SPEECH_END_SILENCE_WINDOWS: usize = 14;
const SEGMENT_PADDING_WINDOWS: usize = 4;
const MIN_SEGMENT_SAMPLES: usize = SAMPLE_RATE / 3;
const MAX_GAP_TO_MERGE_SAMPLES: usize = SAMPLE_RATE / 5;
const MAX_PROGRESSIVE_CHUNK_DURATION_SEC: usize = 30;
const MAX_PROGRESSIVE_CHUNK_SAMPLES: usize = SAMPLE_RATE * MAX_PROGRESSIVE_CHUNK_DURATION_SEC;

pub struct SubtitleWavChunk {
    pub start_time_sec: f64,
    pub wav_data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SampleRange {
    start: usize,
    end: usize,
}

pub fn split_subtitle_wav_into_chunks(wav_data: &[u8]) -> Result<Vec<SubtitleWavChunk>, String> {
    let mut reader = hound::WavReader::new(Cursor::new(wav_data))
        .map_err(|e| format!("Open subtitle WAV for chunking: {e}"))?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != SAMPLE_RATE as u32
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(format!(
            "Unexpected subtitle WAV format: {}ch {}Hz {}-bit {:?}",
            spec.channels, spec.sample_rate, spec.bits_per_sample, spec.sample_format
        ));
    }

    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read subtitle WAV samples: {e}"))?;
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let ranges = detect_speech_ranges(&samples);
    let bounded = if ranges.is_empty() {
        split_long_ranges(
            vec![SampleRange {
                start: 0,
                end: samples.len(),
            }],
            samples.len(),
        )
    } else {
        split_long_ranges(ranges, samples.len())
    };

    bounded
        .into_iter()
        .map(|range| {
            let mut cursor = Cursor::new(Vec::new());
            let mut writer = hound::WavWriter::new(&mut cursor, spec)
                .map_err(|e| format!("Create subtitle chunk WAV: {e}"))?;
            for sample in &samples[range.start..range.end] {
                writer
                    .write_sample(*sample)
                    .map_err(|e| format!("Write subtitle chunk WAV sample: {e}"))?;
            }
            writer
                .finalize()
                .map_err(|e| format!("Finalize subtitle chunk WAV: {e}"))?;
            Ok(SubtitleWavChunk {
                start_time_sec: range.start as f64 / SAMPLE_RATE as f64,
                wav_data: cursor.into_inner(),
            })
        })
        .collect()
}

fn detect_speech_ranges(samples: &[i16]) -> Vec<SampleRange> {
    let window_count = samples.len().div_ceil(WINDOW_SAMPLES);
    let mut ranges = Vec::new();
    let mut in_segment = false;
    let mut segment_start_window = 0usize;
    let mut speech_run = 0usize;
    let mut silence_run = 0usize;

    for window_index in 0..window_count {
        let start = window_index * WINDOW_SAMPLES;
        let end = ((window_index + 1) * WINDOW_SAMPLES).min(samples.len());
        let is_speech = compute_rms(&samples[start..end]) >= SPEECH_RMS_THRESHOLD;

        if is_speech {
            speech_run += 1;
            silence_run = 0;
            if !in_segment && speech_run >= SPEECH_START_WINDOWS {
                let trigger_start = window_index + 1 - speech_run;
                segment_start_window = trigger_start.saturating_sub(SEGMENT_PADDING_WINDOWS);
                in_segment = true;
            }
            continue;
        }

        silence_run += 1;
        speech_run = 0;

        if in_segment && silence_run >= SPEECH_END_SILENCE_WINDOWS {
            let speech_end_window = (window_index + 1).saturating_sub(silence_run);
            let padded_end_window = (speech_end_window + SEGMENT_PADDING_WINDOWS).min(window_count);
            ranges.push(window_range(
                segment_start_window,
                padded_end_window,
                samples.len(),
            ));
            in_segment = false;
        }
    }

    if in_segment {
        ranges.push(window_range(
            segment_start_window,
            window_count,
            samples.len(),
        ));
    }

    merge_ranges(ranges)
}

fn merge_ranges(ranges: Vec<SampleRange>) -> Vec<SampleRange> {
    let mut merged: Vec<SampleRange> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut() {
            let gap = range.start.saturating_sub(previous.end);
            let previous_short = previous.end.saturating_sub(previous.start) < MIN_SEGMENT_SAMPLES;
            let current_short = range.end.saturating_sub(range.start) < MIN_SEGMENT_SAMPLES;
            if gap <= MAX_GAP_TO_MERGE_SAMPLES || previous_short || current_short {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn split_long_ranges(ranges: Vec<SampleRange>, total_samples: usize) -> Vec<SampleRange> {
    let mut split = Vec::new();
    for range in ranges {
        let start = range.start.min(total_samples);
        let end = range.end.min(total_samples);
        if end <= start {
            continue;
        }

        let mut chunk_start = start;
        while chunk_start < end {
            let chunk_end = (chunk_start + MAX_PROGRESSIVE_CHUNK_SAMPLES).min(end);
            split.push(SampleRange {
                start: chunk_start,
                end: chunk_end,
            });
            chunk_start = chunk_end;
        }
    }
    split
}

fn window_range(start_window: usize, end_window: usize, total_samples: usize) -> SampleRange {
    SampleRange {
        start: (start_window * WINDOW_SAMPLES).min(total_samples),
        end: (end_window * WINDOW_SAMPLES).min(total_samples),
    }
}

fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    (sum / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PROGRESSIVE_CHUNK_SAMPLES, SAMPLE_RATE, SampleRange, detect_speech_ranges,
        split_long_ranges,
    };

    fn silence(seconds: f32) -> Vec<i16> {
        vec![0; (seconds * SAMPLE_RATE as f32) as usize]
    }

    fn speech(seconds: f32) -> Vec<i16> {
        vec![(i16::MAX as f32 * 0.2) as i16; (seconds * SAMPLE_RATE as f32) as usize]
    }

    #[test]
    fn detects_multiple_speech_islands() {
        let mut samples = silence(0.3);
        samples.extend(speech(0.5));
        samples.extend(silence(0.5));
        samples.extend(speech(0.6));
        samples.extend(silence(0.3));

        let ranges = detect_speech_ranges(&samples);
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn splits_long_ranges_into_bounded_chunks() {
        let total_samples = MAX_PROGRESSIVE_CHUNK_SAMPLES * 2 + SAMPLE_RATE;
        let ranges = split_long_ranges(
            vec![SampleRange {
                start: 0,
                end: total_samples,
            }],
            total_samples,
        );

        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].end, MAX_PROGRESSIVE_CHUNK_SAMPLES);
        assert_eq!(ranges[1].start, MAX_PROGRESSIVE_CHUNK_SAMPLES);
        assert_eq!(ranges[2].end, total_samples);
    }
}
