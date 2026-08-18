use super::contract::TranslationRegion;

pub(super) fn is_suspiciously_unchanged(region: &TranslationRegion) -> bool {
    let translated = region.translated_segments.join(" ");
    if !super::contract::text_is_source_equivalent(&region.source_text, &translated) {
        return false;
    }
    let has_non_ascii_letters = region
        .source_text
        .chars()
        .any(|character| character.is_alphabetic() && !character.is_ascii());
    let prose_words = region
        .source_text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .count();
    has_non_ascii_letters || prose_words >= 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::screen_translate::contract::{NormalizedBounds, SemanticRole};

    fn region(text: &str) -> TranslationRegion {
        TranslationRegion {
            id: 1,
            member_ids: vec![1],
            member_joins: Vec::new(),
            selections: Vec::new(),
            semantic_role: SemanticRole::Standalone,
            source_text: text.to_string(),
            translated_segments: vec![text.to_string()],
            bounds: NormalizedBounds::from([0, 0, 10, 10]),
            background_color: None,
            text_color: None,
        }
    }

    #[test]
    fn unchanged_names_are_invariant_but_non_ascii_or_prose_requires_fallback() {
        assert!(!is_suspiciously_unchanged(&region("source-1")));
        assert!(is_suspiciously_unchanged(&region("粉田肥門内卜用請")));
        assert!(is_suspiciously_unchanged(&region(
            "This sentence was not translated"
        )));
    }
}
