use super::contract::TranslationRegion;

// Equality is a request-level observation, not a language-quality verdict.
// Names, identifiers, and text already in the target language may be unchanged.
pub(super) fn is_unconfirmed_copy(region: &TranslationRegion) -> bool {
    region.source_text.chars().any(char::is_alphabetic)
        && super::contract::text_is_source_equivalent(
            &region.source_text,
            &region.translated_segments.join(" "),
        )
}

pub(super) fn is_copied_batch(regions: &[TranslationRegion]) -> bool {
    let mut distinct = std::collections::HashSet::new();
    for region in regions {
        if !region.source_text.chars().any(char::is_alphabetic) {
            continue;
        }
        if !is_unconfirmed_copy(region) {
            return false;
        }
        distinct.insert(region.source_text.trim());
    }
    distinct.len() >= 2
}
