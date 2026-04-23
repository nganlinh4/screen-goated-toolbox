use super::types::SubtitleSegmentResult;

const MAX_MICRO_FRAGMENT_SEC: f64 = 0.20;
const MAX_MICRO_JOIN_GAP_SEC: f64 = 0.08;
const MAX_MICRO_PUNCT_CHARS: usize = 3;

const MAX_SHORT_FRAGMENT_SEC: f64 = 1.50;
const MAX_SHORT_JOIN_GAP_SEC: f64 = 0.12;
const MAX_SHORT_FRAGMENT_WORDS: usize = 5;
const MAX_SHORT_FRAGMENT_CHARS: usize = 32;
const MAX_MERGED_SEGMENT_SEC: f64 = 8.50;
const MAX_MERGED_SEGMENT_CHARS: usize = 170;

pub(super) fn sanitize_segments(
    segments: Vec<SubtitleSegmentResult>,
) -> Vec<SubtitleSegmentResult> {
    let mut sanitized: Vec<SubtitleSegmentResult> = Vec::with_capacity(segments.len());
    let mut index = 0;

    while index < segments.len() {
        let segment = segments[index].clone();
        let next = segments.get(index + 1);

        if should_merge_micro_fragment(&segment)
            && let Some(previous) = sanitized.last_mut()
            && can_merge_pair(previous, &segment, MAX_MICRO_JOIN_GAP_SEC)
        {
            merge_into_previous(previous, &segment, next.map(|item| item.start_time));
            index += 1;
            continue;
        }

        if starts_with_continuation(&segment.text)
            && let Some(previous) = sanitized.last_mut()
            && should_merge_short_into_previous(previous, &segment)
        {
            merge_into_previous(previous, &segment, next.map(|item| item.start_time));
            index += 1;
            continue;
        }

        if let Some(next_segment) = next
            && should_merge_short_into_next(&segment, next_segment)
        {
            sanitized.push(merge_pair(&segment, next_segment));
            index += 2;
            continue;
        }

        if next.is_none()
            && let Some(previous) = sanitized.last_mut()
            && should_merge_short_tail_into_previous(previous, &segment)
        {
            merge_into_previous(previous, &segment, None);
            index += 1;
            continue;
        }

        sanitized.push(segment);
        index += 1;
    }

    sanitized
}

fn should_merge_micro_fragment(segment: &SubtitleSegmentResult) -> bool {
    duration(segment) <= MAX_MICRO_FRAGMENT_SEC && is_punctuation_only(&segment.text)
}

fn should_merge_short_into_previous(
    previous: &SubtitleSegmentResult,
    segment: &SubtitleSegmentResult,
) -> bool {
    is_short_merge_candidate(segment)
        && starts_with_continuation(&segment.text)
        && can_merge_pair(previous, segment, MAX_SHORT_JOIN_GAP_SEC)
}

fn should_merge_short_into_next(
    segment: &SubtitleSegmentResult,
    next: &SubtitleSegmentResult,
) -> bool {
    is_short_merge_candidate(segment)
        && !starts_with_continuation(&segment.text)
        && can_merge_pair(segment, next, MAX_SHORT_JOIN_GAP_SEC)
}

fn should_merge_short_tail_into_previous(
    previous: &SubtitleSegmentResult,
    segment: &SubtitleSegmentResult,
) -> bool {
    is_short_merge_candidate(segment)
        && (starts_with_continuation(&segment.text)
            || word_count(segment.text.trim()) <= 4
            || visible_char_count(segment.text.trim()) <= 24)
        && can_merge_pair(previous, segment, MAX_SHORT_JOIN_GAP_SEC)
}

fn is_short_merge_candidate(segment: &SubtitleSegmentResult) -> bool {
    let text = segment.text.trim();
    !text.is_empty()
        && duration(segment) <= MAX_SHORT_FRAGMENT_SEC
        && (word_count(text) <= MAX_SHORT_FRAGMENT_WORDS
            || visible_char_count(text) <= MAX_SHORT_FRAGMENT_CHARS
            || starts_with_continuation(text))
}

fn can_merge_pair(
    left: &SubtitleSegmentResult,
    right: &SubtitleSegmentResult,
    max_join_gap_sec: f64,
) -> bool {
    right.start_time <= left.end_time + max_join_gap_sec
        && right.end_time - left.start_time <= MAX_MERGED_SEGMENT_SEC
        && visible_char_count(&left.text) + visible_char_count(&right.text)
            <= MAX_MERGED_SEGMENT_CHARS
}

fn merge_pair(
    current: &SubtitleSegmentResult,
    next: &SubtitleSegmentResult,
) -> SubtitleSegmentResult {
    SubtitleSegmentResult {
        start_time: current.start_time,
        end_time: next.end_time,
        text: join_text(&current.text, &next.text),
    }
}

fn merge_into_previous(
    previous: &mut SubtitleSegmentResult,
    current: &SubtitleSegmentResult,
    next_start: Option<f64>,
) {
    previous.text = join_text(&previous.text, &current.text);
    let capped_end = next_start
        .map(|value| current.end_time.min(value.max(previous.end_time)))
        .unwrap_or(current.end_time);
    previous.end_time = previous.end_time.max(capped_end);
}

fn join_text(left: &str, right: &str) -> String {
    let left_trimmed = left.trim_end();
    let right_trimmed = right.trim_start();
    if left_trimmed.is_empty() {
        return right_trimmed.to_string();
    }
    if right_trimmed.is_empty() {
        return left_trimmed.to_string();
    }
    if starts_with_inline_punctuation(right_trimmed) {
        format!("{left_trimmed}{right_trimmed}")
    } else {
        format!("{left_trimmed} {right_trimmed}")
    }
}

fn starts_with_continuation(text: &str) -> bool {
    let trimmed = text.trim_start();
    let Some(first) = trimmed.chars().next() else {
        return false;
    };
    first.is_lowercase() || starts_with_inline_punctuation(trimmed)
}

fn starts_with_inline_punctuation(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| matches!(ch, ',' | '.' | ';' | ':' | '\'' | '"' | ')' | ']' | '}'))
}

fn is_punctuation_only(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_MICRO_PUNCT_CHARS
        && trimmed.chars().all(|ch| !ch.is_alphanumeric())
}

fn duration(segment: &SubtitleSegmentResult) -> f64 {
    segment.end_time - segment.start_time
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn visible_char_count(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::{SubtitleSegmentResult, sanitize_segments};

    #[test]
    fn merges_micro_punctuation_into_previous_segment() {
        let sanitized = sanitize_segments(vec![
            SubtitleSegmentResult {
                start_time: 24.926,
                end_time: 31.0,
                text: "Alpha headline".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 31.0,
                end_time: 31.1,
                text: ".".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 31.063,
                end_time: 37.5,
                text: "Beta follow-up sentence".to_string(),
            },
        ]);
        assert_eq!(sanitized.len(), 2);
        assert_eq!(sanitized[0].text, "Alpha headline.");
        assert_eq!(sanitized[0].end_time, 31.063);
    }

    #[test]
    fn merges_short_continuation_into_previous_segment() {
        let sanitized = sanitize_segments(vec![
            SubtitleSegmentResult {
                start_time: 147.5,
                end_time: 154.0,
                text: "Primary clause reaches the cutoff".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 154.0,
                end_time: 155.239,
                text: "and adds one more detail.".to_string(),
            },
        ]);
        assert_eq!(sanitized.len(), 1);
        assert_eq!(
            sanitized[0].text,
            "Primary clause reaches the cutoff and adds one more detail."
        );
        assert_eq!(sanitized[0].end_time, 155.239);
    }

    #[test]
    fn merges_short_standalone_sentence_into_next_segment() {
        let sanitized = sanitize_segments(vec![
            SubtitleSegmentResult {
                start_time: 82.0,
                end_time: 83.489,
                text: "Short setup.".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 83.489,
                end_time: 84.812,
                text: "Next quick beat.".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 84.812,
                end_time: 91.0,
                text: "Longer explanation continues after that.".to_string(),
            },
        ]);
        assert_eq!(sanitized.len(), 2);
        assert_eq!(sanitized[0].text, "Short setup. Next quick beat.");
        assert_eq!(sanitized[0].start_time, 82.0);
        assert_eq!(sanitized[0].end_time, 84.812);
    }

    #[test]
    fn merges_short_tail_into_previous_segment() {
        let sanitized = sanitize_segments(vec![
            SubtitleSegmentResult {
                start_time: 312.85,
                end_time: 315.589,
                text: "Call to action stays on screen.".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 315.589,
                end_time: 317.0,
                text: "Final tag".to_string(),
            },
        ]);
        assert_eq!(sanitized.len(), 1);
        assert_eq!(
            sanitized[0].text,
            "Call to action stays on screen. Final tag"
        );
    }

    #[test]
    fn keeps_longer_brief_sentence_without_clear_merge_signal() {
        let sanitized = sanitize_segments(vec![
            SubtitleSegmentResult {
                start_time: 199.757,
                end_time: 201.269,
                text: "A longer standalone sentence appears here.".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 201.269,
                end_time: 202.0,
                text: "Another longer standalone sentence lands.".to_string(),
            },
            SubtitleSegmentResult {
                start_time: 202.0,
                end_time: 202.651,
                text: "Extra brief sentence.".to_string(),
            },
        ]);
        assert_eq!(sanitized.len(), 3);
        assert_eq!(
            sanitized[1].text,
            "Another longer standalone sentence lands."
        );
    }
}
