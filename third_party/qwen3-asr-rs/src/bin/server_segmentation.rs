use anyhow::Result;
use qwen3_asr_rs::audio;
use qwen3_asr_rs::inference::{AsrInference, DEFAULT_SUBTITLE_MAX_NEW_TOKENS};
use serde::Serialize;

const SAMPLE_RATE: usize = 16_000;
const WINDOW_SAMPLES: usize = 320;
const SPEECH_RMS_THRESHOLD: f32 = 0.015;
const SPEECH_START_WINDOWS: usize = 4;
const SPEECH_END_SILENCE_WINDOWS: usize = 14;
const SEGMENT_PADDING_WINDOWS: usize = 4;
const MIN_SEGMENT_SAMPLES: usize = SAMPLE_RATE / 3;
const MAX_GAP_TO_MERGE_SAMPLES: usize = SAMPLE_RATE / 5;
const MAX_SEGMENT_DURATION_SEC: usize = 30;
const MAX_SEGMENT_SAMPLES: usize = SAMPLE_RATE * MAX_SEGMENT_DURATION_SEC;

#[derive(Clone, Serialize)]
pub struct TimedTranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub struct SegmentedTranscription {
    pub language: String,
    pub duration: f64,
    pub text: String,
    pub segments: Vec<TimedTranscriptSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SampleRange {
    start: usize,
    end: usize,
}

pub fn transcribe_with_segments(
    model: &AsrInference,
    audio_path: &str,
    language: Option<&str>,
) -> Result<SegmentedTranscription> {
    let samples = audio::load_audio(audio_path, SAMPLE_RATE as u32)?;
    let duration = samples.len() as f64 / SAMPLE_RATE as f64;
    let ranges = detect_speech_ranges(&samples);
    let bounded_ranges = if ranges.is_empty() {
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

    if bounded_ranges.is_empty() {
        let result = model.transcribe_samples_with_max_new_tokens(
            &samples,
            language,
            DEFAULT_SUBTITLE_MAX_NEW_TOKENS,
        )?;
        let text = normalize_text(&result.text);
        let segments = if text.is_empty() || duration <= 0.0 {
            Vec::new()
        } else {
            vec![TimedTranscriptSegment {
                start: 0.0,
                end: duration.max(0.1),
                text: text.clone(),
            }]
        };
        return Ok(SegmentedTranscription {
            language: result.language,
            duration,
            text,
            segments,
        });
    }

    let mut detected_language = String::new();
    let mut transcript_texts = Vec::new();
    let mut segments = Vec::new();

    for range in bounded_ranges {
        let result = model.transcribe_samples_with_max_new_tokens(
            &samples[range.start..range.end],
            language,
            DEFAULT_SUBTITLE_MAX_NEW_TOKENS,
        )?;
        if detected_language.is_empty() {
            detected_language = result.language.clone();
        }
        let text = normalize_text(&result.text);
        if text.is_empty() {
            continue;
        }
        transcript_texts.push(text.clone());
        segments.push(TimedTranscriptSegment {
            start: range.start as f64 / SAMPLE_RATE as f64,
            end: (range.end as f64 / SAMPLE_RATE as f64)
                .max(range.start as f64 / SAMPLE_RATE as f64 + 0.1),
            text,
        });
    }

    if segments.is_empty() {
        let result = model.transcribe_samples_with_max_new_tokens(
            &samples,
            language,
            DEFAULT_SUBTITLE_MAX_NEW_TOKENS,
        )?;
        let text = normalize_text(&result.text);
        let segments = if text.is_empty() || duration <= 0.0 {
            Vec::new()
        } else {
            vec![TimedTranscriptSegment {
                start: 0.0,
                end: duration.max(0.1),
                text: text.clone(),
            }]
        };
        return Ok(SegmentedTranscription {
            language: result.language,
            duration,
            text,
            segments,
        });
    }

    Ok(SegmentedTranscription {
        language: detected_language,
        duration,
        text: transcript_texts.join(" "),
        segments,
    })
}

fn detect_speech_ranges(samples: &[f32]) -> Vec<SampleRange> {
    if samples.is_empty() {
        return Vec::new();
    }

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
        let clamped_start = range.start.min(total_samples);
        let clamped_end = range.end.min(total_samples);
        if clamped_end <= clamped_start {
            continue;
        }

        let mut start = clamped_start;
        while start < clamped_end {
            let end = (start + MAX_SEGMENT_SAMPLES).min(clamped_end);
            split.push(SampleRange { start, end });
            start = end;
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

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples.iter().map(|sample| sample * sample).sum::<f32>();
    (sum / samples.len() as f32).sqrt()
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        detect_speech_ranges, split_long_ranges, SampleRange, MAX_GAP_TO_MERGE_SAMPLES,
        MAX_SEGMENT_SAMPLES, SAMPLE_RATE,
    };

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0; (seconds * SAMPLE_RATE as f32) as usize]
    }

    fn speech(seconds: f32) -> Vec<f32> {
        vec![0.2; (seconds * SAMPLE_RATE as f32) as usize]
    }

    #[test]
    fn ignores_empty_audio() {
        assert!(detect_speech_ranges(&[]).is_empty());
        assert!(detect_speech_ranges(&silence(1.0)).is_empty());
    }

    #[test]
    fn keeps_single_speech_island_with_padding() {
        let mut samples = silence(0.5);
        let speech_start = samples.len();
        samples.extend(speech(0.8));
        let speech_end = samples.len();
        samples.extend(silence(0.5));

        let ranges = detect_speech_ranges(&samples);
        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].start < speech_start);
        assert!(ranges[0].end > speech_end);
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
    fn merges_short_gap_fragments() {
        let mut samples = silence(0.3);
        samples.extend(speech(0.25));
        samples.extend(vec![0.0; MAX_GAP_TO_MERGE_SAMPLES / 2]);
        samples.extend(speech(0.25));
        samples.extend(silence(0.3));

        let ranges = detect_speech_ranges(&samples);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn splits_long_ranges_into_bounded_chunks() {
        let total_samples = MAX_SEGMENT_SAMPLES * 2 + SAMPLE_RATE;
        let ranges = split_long_ranges(
            vec![SampleRange {
                start: 0,
                end: total_samples,
            }],
            total_samples,
        );

        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, MAX_SEGMENT_SAMPLES);
        assert_eq!(ranges[1].start, MAX_SEGMENT_SAMPLES);
        assert_eq!(ranges[1].end, MAX_SEGMENT_SAMPLES * 2);
        assert_eq!(ranges[2].start, MAX_SEGMENT_SAMPLES * 2);
        assert_eq!(ranges[2].end, total_samples);
    }

    #[test]
    fn splits_empty_detection_fallback_range() {
        let total_samples = MAX_SEGMENT_SAMPLES + SAMPLE_RATE;
        let ranges = split_long_ranges(
            vec![SampleRange {
                start: 0,
                end: total_samples,
            }],
            total_samples,
        );

        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].end - ranges[0].start, MAX_SEGMENT_SAMPLES);
        assert_eq!(ranges[1].end - ranges[1].start, SAMPLE_RATE);
    }
}
