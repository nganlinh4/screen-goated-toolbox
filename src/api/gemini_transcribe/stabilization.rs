//! Shared delivery policy for speculative transcription, independent of editors.
use unicode_segmentation::UnicodeSegmentation;

pub(super) const REVISION_WORDS: usize = 10;
pub(super) const REVISION_SCALARS: usize = 64;

#[derive(Default)]
pub(super) struct Stabilizer {
    pub(super) visible: String,
    pub(super) overlap_chars: usize,
    frozen: usize,
    previous: Vec<String>,
    snapshot: String,
}

impl Stabilizer {
    pub(super) fn update(&mut self, incoming: &str, final_text: bool) {
        self.overlap_chars = 0;
        let incoming = if !final_text {
            self.snapshot = incoming.to_owned();
            let end = self.overlap_end(incoming);
            self.overlap_chars = incoming[..end].chars().count();
            &incoming[end..]
        } else {
            incoming
        };
        self.frozen = self.frozen.max(mutable_boundary(&self.visible));
        self.visible = reconcile(&self.visible, incoming, self.frozen);
    }

    pub(super) fn finish_segment(&mut self, raw_final: &str) {
        self.previous = vec![
            raw_final.to_owned(),
            std::mem::take(&mut self.snapshot),
            self.visible.clone(),
        ];
        self.visible.clear();
        self.frozen = 0;
    }

    fn overlap_end(&self, incoming: &str) -> usize {
        let new = words(incoming);
        let ranges = word_ranges(incoming);
        if new.is_empty() {
            return 0;
        }
        self.previous
            .iter()
            .map(|text| {
                let old = words(text);
                // Short repetitions are ordinary speech, not strong overlap evidence.
                (0..old.len().saturating_sub(2))
                    .map(|start| {
                        (0..new.len().min(4))
                            .map(|offset| {
                                let count = old[start..]
                                    .iter()
                                    .zip(&new[offset..])
                                    .enumerate()
                                    .take_while(|(i, (a, b))| {
                                        a == b
                                            || (offset + i + 1 == new.len()
                                                && a.starts_with(b.as_str()))
                                    })
                                    .count();
                                let scalars: usize = new[offset..offset + count]
                                    .iter()
                                    .map(|s| s.chars().count())
                                    .sum();
                                // Recompute the carried prefix on every frame. A match
                                // inside old text must never hide unrelated new words.
                                if count >= 3 && scalars >= 12 && start + count == old.len() {
                                    ranges.get(offset + count).map_or(incoming.len(), |r| r.0)
                                } else if offset == 0 && count == new.len() {
                                    incoming.len()
                                } else {
                                    0
                                }
                            })
                            .max()
                            .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0)
    }
}

fn words(text: &str) -> Vec<String> {
    word_ranges(text)
        .into_iter()
        .map(|(a, b)| text[a..b].to_lowercase())
        .collect()
}

fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (i, cluster) in text.grapheme_indices(true) {
        if cluster.chars().any(char::is_alphanumeric) {
            start.get_or_insert(i);
        } else if let Some(a) = start.take() {
            ranges.push((a, i));
        }
    }
    if let Some(a) = start {
        ranges.push((a, text.len()));
    }
    ranges
}

fn mutable_boundary(text: &str) -> usize {
    let words = word_ranges(text);
    let word_boundary = if words.len() > REVISION_WORDS {
        words[words.len() - REVISION_WORDS].0
    } else {
        0
    };
    let mut remaining = REVISION_SCALARS;
    let mut scalar_boundary = text.len();
    for (offset, grapheme) in text.grapheme_indices(true).rev() {
        let count = grapheme.chars().count();
        if count > remaining {
            break;
        }
        remaining -= count;
        scalar_boundary = offset;
    }
    word_boundary.max(scalar_boundary)
}

fn reconcile(old: &str, new: &str, frozen: usize) -> String {
    let left = units(old);
    let right = units(new);
    let mut matches = Vec::new();
    align(&left, &right, 0, 0, &mut matches);
    matches.push((left.len(), right.len()));
    let mut result = String::new();
    let (mut a, mut b, mut offset) = (0, 0, 0);
    for (x, y) in matches {
        if x != a || y != b {
            let source = if offset < frozen {
                &left[a..x]
            } else {
                &right[b..y]
            };
            result.extend(source.iter().copied());
        }
        offset += left[a..x].iter().map(|s| s.len()).sum::<usize>();
        if x < left.len() {
            result.push_str(left[x]);
            offset += left[x].len();
        }
        a = x + 1;
        b = y + 1;
    }
    debug_assert!(result.starts_with(&old[..frozen]));
    result
}

// Keep ordinary lexical replacements whole, so rejecting an old word cannot
// splice its letters into a different new word. Long unspaced runs still stream
// at grapheme granularity instead of becoming an immutable monolithic token.
fn units(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut cursor = 0;
    for (a, b) in word_ranges(text) {
        result.extend(text[cursor..a].graphemes(true));
        let word = &text[a..b];
        if word.chars().count() <= REVISION_SCALARS {
            result.push(word);
        } else {
            result.extend(word.graphemes(true));
        }
        cursor = b;
    }
    result.extend(text[cursor..].graphemes(true));
    result
}

// Linear-space LCS alignment. Matching anchors separate rejected old edits
// from independent new tail content; no whole-event rejection or offset splice.
fn scores(left: &[&str], right: &[&str], reverse: bool) -> Vec<usize> {
    let mut row = vec![0; right.len() + 1];
    for i in 0..left.len() {
        let mut diagonal = 0;
        for j in 0..right.len() {
            let above = row[j + 1];
            let a = if reverse {
                left[left.len() - 1 - i]
            } else {
                left[i]
            };
            let b = if reverse {
                right[right.len() - 1 - j]
            } else {
                right[j]
            };
            row[j + 1] = if a == b {
                diagonal + 1
            } else {
                above.max(row[j])
            };
            diagonal = above;
        }
    }
    row
}

fn align(left: &[&str], right: &[&str], a: usize, b: usize, out: &mut Vec<(usize, usize)>) {
    let prefix = left.iter().zip(right).take_while(|(x, y)| x == y).count();
    out.extend((0..prefix).map(|i| (a + i, b + i)));
    let (left, right, a, b) = (&left[prefix..], &right[prefix..], a + prefix, b + prefix);
    if left.is_empty() || right.is_empty() {
        return;
    }
    if left.len() == 1 {
        if let Some(i) = right.iter().position(|v| *v == left[0]) {
            out.push((a, b + i));
        }
        return;
    }
    let mid = left.len() / 2;
    let forward = scores(&left[..mid], right, false);
    let backward = scores(&left[mid..], right, true);
    let split = (0..=right.len())
        .max_by_key(|&i| forward[i] + backward[right.len() - i])
        .unwrap();
    align(&left[..mid], &right[..split], a, b, out);
    align(&left[mid..], &right[split..], a + mid, b + split, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_correction_does_not_discard_new_tail() {
        let old = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
        let new = "ALPHA beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu";
        let mut state = Stabilizer::default();
        state.update(old, false);
        state.update(new, true);
        assert!(state.visible.starts_with("alpha beta"));
        assert!(state.visible.ends_with("mu nu"));
        assert!(
            old.chars().count()
                - old
                    .chars()
                    .zip(state.visible.chars())
                    .take_while(|(a, b)| a == b)
                    .count()
                <= REVISION_SCALARS
        );
    }

    #[test]
    fn unspaced_text_and_combining_clusters_obey_scalar_bound() {
        let old = "a\u{301}".repeat(100);
        let mut state = Stabilizer::default();
        state.update(&old, false);
        state.update(&"b".repeat(200), true);
        assert!(state.visible.starts_with(&"a\u{301}".repeat(68)));
        assert_eq!(mutable_boundary(&old), "a\u{301}".repeat(68).len());
    }
}
