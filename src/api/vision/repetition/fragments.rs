use std::collections::HashMap;

use super::squeeze;

/// Cache exactly which token boundaries have broken-word evidence after them.
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
            for ch in word.chars() {
                if !haystack.ends_with(ch) {
                    haystack.push(ch);
                }
            }
            ends.push(haystack.len());
            words.push(word);
        }
        let mut first = HashMap::new();
        let mut changes = vec![0i32; words.len() + 2];
        for (index, pair) in words.windows(2).enumerate() {
            let first_index = *first.entry(pair[0].as_str()).or_insert(index);
            let joined = squeeze(&format!("{}{}", pair[0], pair[1]));
            if joined.chars().count() <= pair[0].chars().count() {
                continue;
            }
            let Some(at) = haystack.find(&joined) else {
                continue;
            };
            // Prefixes grow monotonically. The earliest full match is sufficient.
            let lower = ends.partition_point(|&end| end < at + joined.len());
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

#[cfg(test)]
#[path = "fragments/tests.rs"]
mod tests;
