//! Exact non-overlapping previous matches in quadratic time and linear memory.
//! Reuse these spans for anchor and coverage queries instead of rescanning text.

pub(super) struct PreviousMatches {
    spans: Vec<usize>,
    suffix_max: Vec<usize>,
}

impl PreviousMatches {
    pub(super) fn new(chars: &[char]) -> Self {
        let mut common = vec![0; chars.len() + 1];
        let mut spans = vec![0; chars.len()];
        let mut suffix_max = vec![0; chars.len() + 1];
        for start in (0..chars.len()).rev() {
            // Ascending candidates retain the next row's common[candidate + 1].
            for candidate in 0..start {
                common[candidate] = if chars[start] == chars[candidate] {
                    common[candidate + 1] + 1
                } else {
                    0
                };
                spans[start] = spans[start].max(common[candidate].min(start - candidate));
            }
            suffix_max[start] = suffix_max[start + 1].max(spans[start]);
        }
        Self { spans, suffix_max }
    }

    pub(super) fn span(&self, start: usize) -> usize {
        self.spans[start]
    }

    pub(super) fn has_anchor(&self, start: usize, minimum: usize) -> bool {
        self.suffix_max[start] >= minimum
    }

    pub(super) fn coverage(&self, start: usize, end: usize, minimum: usize) -> f32 {
        if end <= start {
            return 0.0;
        }
        let mut covered = 0;
        let mut index = start;
        while index < end {
            let span = self.spans[index];
            if span >= minimum {
                covered += span.min(end - index);
                index += span;
            } else {
                index += 1;
            }
        }
        covered as f32 / (end - start) as f32
    }
}

#[cfg(test)]
#[path = "matching/tests.rs"]
mod tests;
