use crate::overlay::screen_record::ipc::subtitles::audio::MIN_SUBTITLE_DURATION_SEC;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

use super::{
    DIAGNOSTIC_KV_GROWTH_BYTES, DIAGNOSTIC_LOG_EVERY_STEPS, FIXED_TEXT_LAG_SEC, SAMPLE_RATE_SEC,
    normalize_subtitle_text,
};

const MAX_ADJACENT_DUP_GAP_SEC: f64 = 0.25;
const MIN_DUPLICATE_TEXT_LEN: usize = 18;

#[derive(Default)]
pub(super) struct QwenDiagnosticState {
    last_logged_step: usize,
    last_logged_kv_bytes: usize,
    pub(super) session_count: usize,
}

pub(super) struct QwenPulseSnapshot {
    pub(super) step: usize,
    pub(super) total_steps: usize,
    pub(super) stable_time_sec: f64,
    pub(super) pending_duration_sec: f64,
    pub(super) session_audio_samples: usize,
    pub(super) kv_cache_bytes: usize,
    pub(super) kv_cache_dense_bytes: usize,
    pub(super) latency_ms: u64,
    pub(super) fixed_chars: usize,
    pub(super) draft_chars: usize,
}

impl QwenDiagnosticState {
    pub(super) fn begin_session(&mut self) {
        self.session_count += 1;
        self.last_logged_kv_bytes = 0;
    }

    pub(super) fn maybe_log_pulse(&mut self, model_label: &str, pulse: &QwenPulseSnapshot) {
        let step_boundary = pulse.step == 1
            || pulse.step == pulse.total_steps
            || pulse.step.saturating_sub(self.last_logged_step) >= DIAGNOSTIC_LOG_EVERY_STEPS;
        let kv_jump = pulse
            .kv_cache_bytes
            .saturating_sub(self.last_logged_kv_bytes)
            >= DIAGNOSTIC_KV_GROWTH_BYTES;
        if !step_boundary && !kv_jump {
            return;
        }

        crate::log_info!(
            "[SubtitleGen][Qwen] pulse model={} step={}/{} stable_sec={:.1} session_audio_sec={:.1} fixed_chars={} draft_chars={} pending_sec={:.1} kv_cache_mb={:.1} dense_mb={:.1} step_latency_ms={} sessions={}",
            model_label,
            pulse.step,
            pulse.total_steps,
            pulse.stable_time_sec,
            pulse.session_audio_samples as f64 / SAMPLE_RATE_SEC,
            pulse.fixed_chars,
            pulse.draft_chars,
            pulse.pending_duration_sec,
            pulse.kv_cache_bytes as f64 / (1024.0 * 1024.0),
            pulse.kv_cache_dense_bytes as f64 / (1024.0 * 1024.0),
            pulse.latency_ms,
            self.session_count
        );
        self.last_logged_step = pulse.step;
        self.last_logged_kv_bytes = pulse.kv_cache_bytes;
    }
}

pub(super) fn finalize_visible_segments(
    committed_segments: Vec<CompactSubtitleSegment>,
    visible_tail_segments: Vec<CompactSubtitleSegment>,
) -> Vec<CompactSubtitleSegment> {
    let mut retained_committed_segments: Vec<_> = committed_segments
        .into_iter()
        .filter(|committed| {
            !visible_tail_segments
                .iter()
                .any(|tail| segments_overlap(committed, tail))
        })
        .collect();
    retained_committed_segments.extend(visible_tail_segments);
    retained_committed_segments.sort_by(|left, right| left.start_time.total_cmp(&right.start_time));
    dedupe_adjacent_segments(retained_committed_segments)
}

pub(super) fn build_visible_progress_segments(
    committed_segments: &[CompactSubtitleSegment],
    visible_tail_segments: &[CompactSubtitleSegment],
    window_progress_segments: &[CompactSubtitleSegment],
) -> Vec<CompactSubtitleSegment> {
    let mut merged = committed_segments.to_vec();
    merged.extend_from_slice(visible_tail_segments);
    merged.extend(
        window_progress_segments
            .iter()
            .filter(|segment| {
                !visible_tail_segments
                    .iter()
                    .any(|previous| segments_overlap(previous, segment))
            })
            .cloned(),
    );
    merged
}

fn segments_overlap(left: &CompactSubtitleSegment, right: &CompactSubtitleSegment) -> bool {
    left.end_time > right.start_time && left.start_time < right.end_time
}

fn dedupe_adjacent_segments(segments: Vec<CompactSubtitleSegment>) -> Vec<CompactSubtitleSegment> {
    let mut deduped = Vec::with_capacity(segments.len());
    for segment in segments {
        push_deduped_segment(&mut deduped, segment);
    }
    deduped
}

fn push_deduped_segment(
    deduped: &mut Vec<CompactSubtitleSegment>,
    segment: CompactSubtitleSegment,
) {
    let Some(previous) = deduped.last_mut() else {
        deduped.push(segment);
        return;
    };
    if segment.start_time - previous.end_time > MAX_ADJACENT_DUP_GAP_SEC {
        deduped.push(segment);
        return;
    }

    let previous_norm = normalized_compare_text(&previous.text);
    let segment_norm = normalized_compare_text(&segment.text);
    if previous_norm.len() < MIN_DUPLICATE_TEXT_LEN || segment_norm.len() < MIN_DUPLICATE_TEXT_LEN {
        deduped.push(segment);
        return;
    }

    if segment_norm.starts_with(&previous_norm) {
        previous.end_time = segment.end_time.max(previous.end_time);
        previous.text = segment.text;
        return;
    }
    if segment_norm.contains(&previous_norm) {
        previous.end_time = segment.end_time.max(previous.end_time);
        previous.text = segment.text;
        return;
    }
    if previous_norm.contains(&segment_norm) {
        previous.end_time = segment.end_time.max(previous.end_time);
        return;
    }
    deduped.push(segment);
}

fn normalized_compare_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            normalized.extend(ch.to_lowercase());
        } else if ch.is_whitespace() && !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }
    normalized.trim().to_string()
}

#[derive(Default)]
pub(super) struct StreamingSubtitleAssembler {
    pub(super) segments: Vec<CompactSubtitleSegment>,
    emitted_text: String,
    pending_start_time: Option<f64>,
}

impl StreamingSubtitleAssembler {
    pub(super) fn observe_text(
        &mut self,
        full_text: &str,
        speech_start_hint: Option<f64>,
        fallback_start_time: f64,
        current_time: f64,
        force_sentence_emit: bool,
    ) -> Result<(), String> {
        let pending_raw = self.pending_raw(full_text)?;
        if !pending_raw.trim().is_empty() && self.pending_start_time.is_none() {
            self.pending_start_time = Some(speech_start_hint.unwrap_or(fallback_start_time));
        }

        if force_sentence_emit {
            return Ok(());
        }

        loop {
            let pending_raw = self.pending_raw(full_text)?;
            let Some(boundary) = find_sentence_boundary(pending_raw, true) else {
                break;
            };
            let emit_raw = &pending_raw[..boundary];
            let remaining_raw = &pending_raw[boundary..];
            let window_start = self.pending_start_time.unwrap_or(current_time);
            let total_weight = measurable_text_len(pending_raw).max(1) as f64;
            let emit_weight = measurable_text_len(emit_raw).max(1) as f64;
            let emit_end = if remaining_raw.trim().is_empty() {
                current_time
            } else {
                window_start + (current_time - window_start) * (emit_weight / total_weight)
            };
            self.emit_raw(emit_raw, emit_end.max(window_start));
            if !remaining_raw.trim().is_empty() {
                self.pending_start_time = Some(emit_end.max(window_start));
            }
        }

        Ok(())
    }

    pub(super) fn flush_pending_from_text(
        &mut self,
        full_text: &str,
        end_time: f64,
    ) -> Result<bool, String> {
        let pending_raw = self.pending_raw(full_text)?;
        if pending_raw.trim().is_empty() {
            self.pending_start_time = None;
            return Ok(false);
        }

        self.emit_raw(
            pending_raw,
            end_time.max(self.pending_start_time.unwrap_or(end_time)),
        );
        Ok(true)
    }

    pub(super) fn progress_segments(
        &self,
        full_text: &str,
        current_time: f64,
    ) -> Vec<CompactSubtitleSegment> {
        let mut segments = self.segments.clone();
        let pending_raw = match self.pending_raw(full_text) {
            Ok(raw) => raw,
            Err(_) => return segments,
        };
        let text = normalize_subtitle_text(pending_raw);
        let Some(start_time) = self.pending_start_time else {
            return segments;
        };
        if text.is_empty() {
            return segments;
        }

        segments.push(CompactSubtitleSegment {
            start_time: start_time.max(0.0),
            end_time: current_time.max(start_time + MIN_SUBTITLE_DURATION_SEC),
            text,
        });
        segments
    }

    pub(super) fn pending_duration(&self, current_time: f64) -> f64 {
        self.pending_start_time
            .map(|start| (current_time - start).max(0.0))
            .unwrap_or(0.0)
    }

    fn emit_raw(&mut self, raw_text: &str, end_time: f64) {
        let start_time = self.pending_start_time.unwrap_or(end_time);
        let text = normalize_subtitle_text(raw_text);
        self.emitted_text.push_str(raw_text);
        if !text.is_empty() {
            self.segments.push(CompactSubtitleSegment {
                start_time: start_time.max(0.0),
                end_time: end_time.max(start_time + MIN_SUBTITLE_DURATION_SEC),
                text,
            });
        }
        self.pending_start_time = None;
    }

    fn pending_raw<'a>(&self, full_text: &'a str) -> Result<&'a str, String> {
        full_text.strip_prefix(&self.emitted_text).ok_or_else(|| {
            format!(
                "Qwen Local subtitle streaming lost stable-text prefix alignment (emitted_chars={}, current_chars={})",
                self.emitted_text.chars().count(),
                full_text.chars().count()
            )
        })
    }
}

pub(super) fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples
        .iter()
        .map(|&sample| (sample as f64 / 32768.0).powi(2))
        .sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

pub(super) fn stable_commit_time(current_time_sec: f64) -> f64 {
    (current_time_sec - FIXED_TEXT_LAG_SEC).max(0.0)
}

fn measurable_text_len(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

fn find_sentence_boundary(text: &str, require_following_text: bool) -> Option<usize> {
    for (index, ch) in text.char_indices() {
        if !matches!(ch, '.' | '!' | '?' | '…') {
            continue;
        }

        let mut end = index + ch.len_utf8();
        while let Some(tail_ch) = text[end..].chars().next() {
            if matches!(tail_ch, '"' | '\'' | '”' | '’' | ')' | ']' | '}' | '»')
                || tail_ch.is_whitespace()
            {
                end += tail_ch.len_utf8();
                continue;
            }
            break;
        }

        if require_following_text && text[end..].trim().is_empty() {
            continue;
        }

        return Some(end);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{CompactSubtitleSegment, dedupe_adjacent_segments, find_sentence_boundary};

    #[test]
    fn sentence_boundary_waits_for_following_text_when_requested() {
        assert_eq!(find_sentence_boundary("Hello world.", true), None);
        assert_eq!(
            find_sentence_boundary("Hello world. Next line", true),
            Some(13)
        );
    }

    #[test]
    fn sentence_boundary_consumes_quotes_and_whitespace() {
        let text = "Hello world.\"  Next";
        assert_eq!(find_sentence_boundary(text, true), Some(15));
    }

    #[test]
    fn sentence_boundary_supports_terminal_emit_when_following_text_not_required() {
        assert_eq!(find_sentence_boundary("Hello world!", false), Some(12));
    }

    #[test]
    fn dedupe_adjacent_segments_merges_prefix_replay() {
        let deduped = dedupe_adjacent_segments(vec![
            CompactSubtitleSegment {
                start_time: 1.0,
                end_time: 2.0,
                text: "Now you may be wondering whether this is actually.".to_string(),
            },
            CompactSubtitleSegment {
                start_time: 2.0,
                end_time: 5.0,
                text: "Now you may be wondering whether this is actually useful.".to_string(),
            },
        ]);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].start_time, 1.0);
        assert_eq!(
            deduped[0].text,
            "Now you may be wondering whether this is actually useful."
        );
    }

    #[test]
    fn dedupe_adjacent_segments_drops_contained_replay() {
        let deduped = dedupe_adjacent_segments(vec![
            CompactSubtitleSegment {
                start_time: 1.0,
                end_time: 4.0,
                text: "If you work through the algebra and you're left with just log of x."
                    .to_string(),
            },
            CompactSubtitleSegment {
                start_time: 4.0,
                end_time: 4.5,
                text: "You're left with just log of x.".to_string(),
            },
        ]);
        assert_eq!(deduped.len(), 1);
        assert!(deduped[0].end_time >= 4.5);
    }

    #[test]
    fn dedupe_adjacent_segments_keeps_partial_overlap_when_it_adds_new_text() {
        let deduped = dedupe_adjacent_segments(vec![
            CompactSubtitleSegment {
                start_time: 117.704,
                end_time: 122.0,
                text: "That'll give you multiplication, and once you have multiplication, you can define powers and so on. If you want to get.".to_string(),
            },
            CompactSubtitleSegment {
                start_time: 122.0,
                end_time: 126.749,
                text: "You can define powers and so on if you want to get trigonometric functions.".to_string(),
            },
        ]);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].end_time, 122.0);
        assert_eq!(
            deduped[1].text,
            "You can define powers and so on if you want to get trigonometric functions."
        );
    }

    #[test]
    fn dedupe_adjacent_segments_merges_shifted_growth_replay() {
        let deduped = dedupe_adjacent_segments(vec![
            CompactSubtitleSegment {
                start_time: 221.0,
                end_time: 222.0,
                text: "is a possible practical angle if every ordinary.".to_string(),
            },
            CompactSubtitleSegment {
                start_time: 222.0,
                end_time: 226.0,
                text: "There is a possible practical angle if every ordinary formula can be."
                    .to_string(),
            },
        ]);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].start_time, 221.0);
        assert_eq!(
            deduped[0].text,
            "There is a possible practical angle if every ordinary formula can be."
        );
    }
}
