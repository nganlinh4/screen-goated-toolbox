//! Shared lexical terms for bounded research ranking.

use std::collections::HashSet;

pub(super) fn tokens(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 2)
        .collect()
}

/// Short terms must begin an alphanumeric run; longer terms keep substring matching.
pub(super) fn match_offset(term: &str, lower_text: &str) -> Option<usize> {
    lower_text.match_indices(term).find_map(|(offset, _)| {
        (term.chars().count() > 2
            || lower_text[..offset]
                .chars()
                .next_back()
                .is_none_or(|previous| !previous.is_alphanumeric()))
        .then_some(offset)
    })
}

/// Exact short-token matches outrank prefixes while prefixes remain eligible.
pub(super) fn exact_short_match(term: &str, lower_text: &str) -> bool {
    term.chars().count() == 2
        && lower_text.match_indices(term).any(|(offset, _)| {
            lower_text[..offset]
                .chars()
                .next_back()
                .is_none_or(|previous| !previous.is_alphanumeric())
                && lower_text[offset + term.len()..]
                    .chars()
                    .next()
                    .is_none_or(|next| !next.is_alphanumeric())
        })
}

#[cfg(test)]
#[path = "query_terms_tests.rs"]
mod tests;
