use serde::Deserialize;

use crate::overlay::screen_record::ipc::subtitles::audio::MIN_SUBTITLE_DURATION_SEC;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

use super::gemini_stream::extract_complete_segment_object_strings;
use super::normalize_subtitle_text;

pub const GEMINI_DURATION_TOLERANCE_MS: i64 = 750;

#[derive(Debug, Deserialize)]
pub struct GeminiSubtitlePayload {
    pub segments: Vec<GeminiTimedSegment>,
}

#[derive(Debug, Deserialize)]
pub struct GeminiTimedSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

pub fn parse_gemini_segments_from_text(
    text: &str,
    clip_duration_sec: f64,
) -> Result<Vec<CompactSubtitleSegment>, String> {
    let payload: GeminiSubtitlePayload = serde_json::from_str(text)
        .map_err(|e| format!("Decode Gemini structured subtitle JSON: {e}"))?;
    validate_segments(payload, clip_duration_sec)
}

pub fn parse_streamed_segment_prefix(
    streamed_text: &str,
    clip_duration_sec: f64,
) -> Result<Vec<CompactSubtitleSegment>, String> {
    let objects = extract_complete_segment_object_strings(streamed_text)?;
    if objects.is_empty() {
        return Ok(Vec::new());
    }
    let mut segments = Vec::with_capacity(objects.len());
    for object in objects {
        let segment: GeminiTimedSegment = serde_json::from_str(&object)
            .map_err(|e| format!("Decode Gemini streamed subtitle segment: {e}"))?;
        segments.push(segment);
    }
    validate_segments(GeminiSubtitlePayload { segments }, clip_duration_sec)
}

pub fn validate_segments(
    payload: GeminiSubtitlePayload,
    clip_duration_sec: f64,
) -> Result<Vec<CompactSubtitleSegment>, String> {
    if payload.segments.is_empty() {
        return Err("Gemini subtitle response returned no segments".to_string());
    }

    let clip_duration_ms = (clip_duration_sec * 1000.0).round() as i64;
    let mut previous_end_ms = 0i64;
    let mut compact_segments = Vec::with_capacity(payload.segments.len());

    for (index, segment) in payload.segments.into_iter().enumerate() {
        let text = normalize_subtitle_text(&segment.text);
        if text.is_empty() {
            return Err(format!(
                "Gemini subtitle response segment {} had empty text",
                index + 1
            ));
        }
        if segment.start_ms < 0 || segment.end_ms <= segment.start_ms {
            return Err(format!(
                "Gemini subtitle response segment {} had invalid timestamps",
                index + 1
            ));
        }
        if index > 0 && segment.start_ms < previous_end_ms {
            return Err(format!(
                "Gemini subtitle response segment {} overlapped the previous segment",
                index + 1
            ));
        }
        if segment.start_ms > clip_duration_ms + GEMINI_DURATION_TOLERANCE_MS
            || segment.end_ms > clip_duration_ms + GEMINI_DURATION_TOLERANCE_MS
        {
            return Err(format!(
                "Gemini subtitle response segment {} exceeded clip duration",
                index + 1
            ));
        }

        previous_end_ms = segment.end_ms;
        compact_segments.push(CompactSubtitleSegment {
            start_time: segment.start_ms as f64 / 1000.0,
            end_time: (segment.end_ms as f64 / 1000.0)
                .max(segment.start_ms as f64 / 1000.0 + MIN_SUBTITLE_DURATION_SEC),
            text,
        });
    }

    Ok(compact_segments)
}

#[cfg(test)]
mod tests {
    use super::{
        GEMINI_DURATION_TOLERANCE_MS, GeminiSubtitlePayload, GeminiTimedSegment, validate_segments,
    };

    #[test]
    fn rejects_overlapping_segments() {
        let error = validate_segments(
            GeminiSubtitlePayload {
                segments: vec![
                    GeminiTimedSegment {
                        start_ms: 0,
                        end_ms: 1000,
                        text: "Alpha".to_string(),
                    },
                    GeminiTimedSegment {
                        start_ms: 900,
                        end_ms: 1800,
                        text: "Beta".to_string(),
                    },
                ],
            },
            2.0,
        )
        .expect_err("expected overlap rejection");
        assert!(error.contains("overlapped"));
    }

    #[test]
    fn rejects_empty_text_segments() {
        let error = validate_segments(
            GeminiSubtitlePayload {
                segments: vec![GeminiTimedSegment {
                    start_ms: 0,
                    end_ms: 1000,
                    text: "   ".to_string(),
                }],
            },
            1.5,
        )
        .expect_err("expected empty text rejection");
        assert!(error.contains("empty text"));
    }

    #[test]
    fn rejects_segments_far_beyond_duration() {
        let error = validate_segments(
            GeminiSubtitlePayload {
                segments: vec![GeminiTimedSegment {
                    start_ms: 0,
                    end_ms: 2500 + GEMINI_DURATION_TOLERANCE_MS,
                    text: "Alpha".to_string(),
                }],
            },
            1.0,
        )
        .expect_err("expected duration rejection");
        assert!(error.contains("exceeded clip duration"));
    }

    #[test]
    fn accepts_valid_segments() {
        let segments = validate_segments(
            GeminiSubtitlePayload {
                segments: vec![
                    GeminiTimedSegment {
                        start_ms: 0,
                        end_ms: 1200,
                        text: "Alpha".to_string(),
                    },
                    GeminiTimedSegment {
                        start_ms: 1200,
                        end_ms: 2500,
                        text: "Beta".to_string(),
                    },
                ],
            },
            3.0,
        )
        .expect("expected valid Gemini segments");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Alpha");
        assert_eq!(segments[1].start_time, 1.2);
    }
}
