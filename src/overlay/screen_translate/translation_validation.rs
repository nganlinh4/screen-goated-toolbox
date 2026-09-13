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

/// Reuse a needed structural retry to review copies, without making every
/// correctly preserved name trigger another request.
pub(super) fn copies_for_recovery(
    regions: &[TranslationRegion],
    requested: usize,
) -> Vec<TranslationRegion> {
    if regions.len() >= requested && !is_copied_batch(regions) {
        return Vec::new();
    }
    regions
        .iter()
        .filter(|region| is_unconfirmed_copy(region))
        .cloned()
        .collect()
}

/// Prefer the original spelling and formatting when review is source-equivalent.
/// Equality does not justify introducing identifier edits.
pub(super) fn retain_equivalent_draft(
    region: TranslationRegion,
    drafts: &[TranslationRegion],
) -> TranslationRegion {
    drafts
        .iter()
        .find(|draft| {
            draft.id == region.id
                && super::contract::text_is_source_equivalent(
                    &draft.translated_segments.join(" "),
                    &region.translated_segments.join(" "),
                )
        })
        .cloned()
        .unwrap_or(region)
}
