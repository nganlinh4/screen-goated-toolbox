//! Re-deal the words of a finalized clip transcript across the subtitle cues
//! according to how much speech each cue actually carries.
//!
//! The Gemini Translate live stream delivers text and audio at different rates,
//! so the per-region text delta over-fills early (dense) cues while later cues
//! run dry — the subtitle text "leads" the narration audio. We keep every cue's
//! audio-synced time box and instead redistribute the words by timing: each cue
//! receives a share of the full transcript proportional to its **speech-active
//! duration** (how many seconds of actual voice fall inside it), in reading
//! order. A dense cue's trailing words therefore cascade forward into the
//! following cues, lining the words up with where they are spoken — with no
//! reliance on punctuation.

/// Split `full_text` (whitespace-delimited words) across cues so each cue's word
/// count is proportional to its `weight` (its speech-active duration in seconds),
/// preserving word order. Returns exactly one `String` per cue.
///
/// When there are at least as many words as cues, **every cue keeps at least one
/// word** so no audio take ends up with an empty (dropped) subtitle; a dense
/// cue's trailing words simply cascade forward into the following cues.
pub(super) fn redistribute_words_by_weight(weights: &[f64], full_text: &str) -> Vec<String> {
    let count = weights.len();
    if count == 0 {
        return Vec::new();
    }
    let words: Vec<&str> = full_text.split_whitespace().collect();
    let total_words = words.len();
    if total_words == 0 {
        return vec![String::new(); count];
    }

    let weights: Vec<f64> = weights.iter().map(|w| w.max(0.0)).collect();
    let total_weight: f64 = weights.iter().sum();
    let guarantee_each = total_words >= count;

    let mut result = Vec::with_capacity(count);
    let mut cursor = 0usize;
    let mut cumulative_weight = 0.0f64;
    for (index, weight) in weights.iter().enumerate() {
        cumulative_weight += weight;
        let next_cursor = if index + 1 == count {
            // Last cue always absorbs the remaining words so none are dropped.
            total_words
        } else {
            let proportional = if total_weight > 0.0 {
                ((cumulative_weight / total_weight) * total_words as f64).round() as usize
            } else {
                // Degenerate all-zero weights: fall back to an even split.
                ((index + 1) * total_words) / count
            };
            if guarantee_each {
                // Keep >=1 word here and leave >=1 for each remaining cue.
                proportional.clamp(cursor + 1, total_words - (count - index - 1))
            } else {
                proportional.clamp(cursor, total_words)
            }
        };
        result.push(words[cursor..next_cursor].join(" "));
        cursor = next_cursor;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::redistribute_words_by_weight;

    fn word_counts(weights: &[f64], text: &str) -> Vec<usize> {
        redistribute_words_by_weight(weights, text)
            .iter()
            .map(|line| line.split_whitespace().count())
            .collect()
    }

    #[test]
    fn equal_weights_split_evenly_and_cascade_forward() {
        // Four cues that carry equal speech and 20 words → 5 each, in order.
        let text = (1..=20)
            .map(|n| format!("w{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(word_counts(&[1.0, 1.0, 1.0, 1.0], &text), vec![5, 5, 5, 5]);
        let lines = redistribute_words_by_weight(&[1.0, 1.0, 1.0, 1.0], &text);
        assert_eq!(lines[0], "w1 w2 w3 w4 w5");
        assert_eq!(lines[3], "w16 w17 w18 w19 w20");
    }

    #[test]
    fn share_is_proportional_to_speech_weight() {
        // A cue carrying 3s of speech holds ~3x the words of a 1s cue.
        assert_eq!(word_counts(&[3.0, 1.0], "a b c d"), vec![3, 1]);
    }

    #[test]
    fn dense_cue_never_empties_short_cues() {
        // One speech-heavy cue + three near-silent ones, 10 words: the dense cue
        // keeps its share, each short cue still gets >=1 word (no dropped
        // subtitles), and the trailing words cascade forward.
        let counts = word_counts(
            &[10.0, 0.2, 0.2, 0.6],
            "one two three four five six seven eight nine ten",
        );
        assert!(counts.iter().all(|&c| c >= 1), "no cue may be empty: {counts:?}");
        assert_eq!(counts.iter().sum::<usize>(), 10);
        assert_eq!(counts, vec![7, 1, 1, 1]);
    }

    #[test]
    fn no_words_are_lost_or_duplicated() {
        let lines = redistribute_words_by_weight(&[2.3, 0.1, 6.6], "one two three four five six seven");
        let rejoined = lines
            .iter()
            .filter(|line| !line.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(rejoined, "one two three four five six seven");
    }

    #[test]
    fn empty_transcript_yields_one_empty_string_per_cue() {
        assert_eq!(
            redistribute_words_by_weight(&[1.0, 1.0], "   "),
            vec![String::new(), String::new()],
        );
    }

    #[test]
    fn single_cue_keeps_the_whole_transcript() {
        assert_eq!(
            redistribute_words_by_weight(&[5.0], "hello there world"),
            vec!["hello there world".to_string()],
        );
    }

    #[test]
    fn zero_weights_fall_back_to_even_split() {
        assert_eq!(word_counts(&[0.0, 0.0], "a b c d"), vec![2, 2]);
    }

    #[test]
    fn no_cues_returns_empty() {
        assert!(redistribute_words_by_weight(&[], "a b c").is_empty());
    }
}
