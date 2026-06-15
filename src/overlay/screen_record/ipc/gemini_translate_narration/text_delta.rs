use crate::api::realtime_audio::s2s::utils::raw_suffix_prefix_overlap;

// NOTE: This narration merge deliberately differs from the canonical
// `merge_segment_text` in `api::realtime_audio::s2s::utils`. It is NOT a thin
// wrapper around the canonical merge — the observable behavior is different and
// intentionally so for the Gemini Translate live-translate transcript stream:
//   * No `MIN_TEXT_OVERLAP_CHARS` guard: it accepts ANY raw suffix/prefix
//     overlap (via `raw_suffix_prefix_overlap`), so even a 1-2 char overlap is
//     trimmed instead of duplicated.
//   * Extra `contains()` early-out: if `incoming` already appears anywhere in
//     `current`, the update is dropped entirely (not just on a trailing match).
//   * Always inserts a single space before the residual and `trim_start()`s it,
//     rather than the canonical's punctuation/whitespace-aware spacing.
// Only the pure string-overlap primitive is shared with the canonical code.
pub(super) fn merge_text(existing: &mut String, incoming: &str) {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return;
    }
    let current = existing.trim();
    if current.is_empty() || incoming.starts_with(current) {
        *existing = incoming.to_string();
    } else if current.ends_with(incoming) || current.contains(incoming) {
    } else {
        let overlap = raw_suffix_prefix_overlap(current, incoming);
        if overlap < incoming.len() {
            existing.push(' ');
            existing.push_str(incoming[overlap..].trim_start());
        }
    }
}

pub(super) fn take_text_delta(text: &str, previous: &mut String) -> String {
    let current = text.trim();
    let last = previous.trim();
    let mut should_update_previous = !current.is_empty();
    let delta = if current.is_empty() || current == last {
        String::new()
    } else if last.contains(current) {
        should_update_previous = false;
        String::new()
    } else if last.is_empty() {
        current.to_string()
    } else if let Some(suffix) = current.strip_prefix(last) {
        suffix.trim().to_string()
    } else {
        let overlap = raw_suffix_prefix_overlap(last, current);
        current[overlap..].trim().to_string()
    };
    if should_update_previous {
        *previous = current.to_string();
    }
    delta
}

pub(super) fn nonempty_text(delta: String, full: &str, fallback: &str) -> String {
    if !delta.trim().is_empty() {
        delta
    } else if !full.trim().is_empty() {
        full.trim().to_string()
    } else {
        fallback.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{merge_text, take_text_delta};

    // Captures the intentionally-different narration merge semantics so future
    // refactors don't silently converge it onto the canonical
    // `merge_segment_text` behavior.

    #[test]
    fn merge_text_replaces_when_incoming_extends_current() {
        let mut text = String::from("hello");
        merge_text(&mut text, "hello world");
        assert_eq!(text, "hello world");
    }

    #[test]
    fn merge_text_trims_short_overlap_without_min_guard() {
        // A 1-char raw overlap ("o"/"o") is trimmed here; the canonical merge
        // would ignore it (below MIN_TEXT_OVERLAP_CHARS) and duplicate it.
        let mut text = String::from("foo");
        merge_text(&mut text, "obar");
        assert_eq!(text, "foo bar");
    }

    #[test]
    fn merge_text_drops_incoming_contained_in_current() {
        // `contains()` early-out: canonical only checks a trailing match.
        let mut text = String::from("the quick brown fox");
        merge_text(&mut text, "quick brown");
        assert_eq!(text, "the quick brown fox");
    }

    #[test]
    fn merge_text_always_inserts_space_before_residual() {
        let mut text = String::from("alpha");
        merge_text(&mut text, "beta");
        assert_eq!(text, "alpha beta");
    }

    #[test]
    fn take_text_delta_returns_new_suffix() {
        let mut previous = String::from("hello");
        let delta = take_text_delta("hello world", &mut previous);
        assert_eq!(delta, "world");
        assert_eq!(previous, "hello world");
    }

    #[test]
    fn take_text_delta_uses_raw_overlap_for_partial_continuation() {
        let mut previous = String::from("good mor");
        let delta = take_text_delta("morning everyone", &mut previous);
        assert_eq!(delta, "ning everyone");
        assert_eq!(previous, "morning everyone");
    }
}
