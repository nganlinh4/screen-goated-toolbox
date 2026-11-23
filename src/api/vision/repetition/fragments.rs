use std::collections::HashMap;

/// Cache exactly which token boundaries have broken-word evidence after them.
///
/// Evidence is two adjacent tokens that, rejoined, read as part of one
/// earlier word: `Screensho` + `t` inside `Screenshot`. Both conditions
/// matter. Text is compared as written: folding doubled characters made `11`
/// read as `1` and `100` as `10`, so every numbered list contained its own
/// "broken words". And the rejoined text must sit inside a single earlier
/// word: a match across an earlier word boundary (`11` `12.` then `112.`) is
/// two whole words read again, which is what correct rows do.
pub(super) struct FragmentEvidence {
    offsets: Vec<usize>,
    boundaries: Vec<bool>,
}

impl FragmentEvidence {
    pub(super) fn new(text: &str) -> Self {
        let mut offsets = Vec::new();
        let mut words = Vec::new();
        let mut ends = vec![0];
        let mut haystack = String::new();
        for token in text.split_whitespace() {
            offsets.push(token.as_ptr() as usize - text.as_ptr() as usize);
            let word = token.to_lowercase();
            haystack.push_str(&word);
            ends.push(haystack.len());
            words.push(word);
        }
        let mut first = HashMap::new();
        let mut changes = vec![0i32; words.len() + 2];
        for (index, pair) in words.windows(2).enumerate() {
            let first_index = *first.entry(pair[0].as_str()).or_insert(index);
            let joined = rejoin(&pair[0], &pair[1]);
            if joined.chars().count() <= pair[0].chars().count() {
                continue;
            }
            let Some(word) = first_word_containing(&haystack, &ends, &joined) else {
                continue;
            };
            // Evidence holds for onsets after that earlier word and up to the
            // pair, as long as the pair's first half is not itself an earlier word.
            let lower = word + 1;
            let upper = index.min(first_index);
            if lower <= upper {
                changes[lower] += 1;
                changes[upper + 1] -= 1;
            }
        }
        let mut active = 0;
        let boundaries = changes
            .into_iter()
            .map(|change| {
                active += change;
                active > 0
            })
            .collect();
        Self {
            offsets,
            boundaries,
        }
    }

    pub(super) fn at(&self, onset: usize) -> bool {
        self.boundaries[self.offsets.partition_point(|&offset| offset < onset)]
    }
}

/// Joins two tokens as the halves of one broken word. The defect can repeat
/// the character it broke at (`aivqvn` + `n/arz`), so a shared seam character
/// is written once.
fn rejoin(left: &str, right: &str) -> String {
    let mut rest = right.chars();
    if left
        .chars()
        .last()
        .is_some_and(|last| right.starts_with(last))
    {
        rest.next();
    }
    let mut joined = left.to_string();
    joined.extend(rest);
    joined
}

/// Index of the earliest word in `haystack` (words concatenated, ending at
/// `ends[1..]`) that contains `needle` entirely.
fn first_word_containing(haystack: &str, ends: &[usize], needle: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = haystack[from..].find(needle) {
        let at = from + found;
        let word = ends.partition_point(|&end| end <= at) - 1;
        if ends[word + 1] >= at + needle.len() {
            return Some(word);
        }
        // Matches may overlap, so resume one character later.
        from = at + haystack[at..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

#[cfg(test)]
#[path = "fragments/tests.rs"]
mod tests;
