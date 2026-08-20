use super::contract::{RecognitionEvidence, TranslationRegion};

pub(super) fn is_suspiciously_unchanged(
    region: &TranslationRegion,
    recognition: RecognitionEvidence,
) -> bool {
    let translated = region.translated_segments.join(" ");
    if !super::contract::text_is_source_equivalent(&region.source_text, &translated) {
        return false;
    }
    if is_invariant_text(&region.source_text) {
        return false;
    }
    let alphanumeric_characters = region
        .source_text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    if alphanumeric_characters <= 2 {
        return false;
    }
    let alphabetic_characters = region
        .source_text
        .chars()
        .filter(|character| character.is_alphabetic())
        .count();
    let prose_words = region
        .source_text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .count();
    let uncertain_short_observation = recognition.selected_confidence < 0.72
        && recognition.competing_confidence == 0.0
        && prose_words <= 1
        && alphabetic_characters < 12;
    !uncertain_short_observation && (prose_words >= 4 || alphabetic_characters >= 6)
}

fn is_invariant_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.contains("://")
        || trimmed.starts_with("www.")
        || (trimmed.contains('@') && !trimmed.contains(char::is_whitespace))
    {
        return true;
    }
    let whitespace_tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if trimmed.starts_with('#') && whitespace_tokens.iter().all(|token| token.starts_with('#')) {
        return true;
    }
    let has_ascii_uppercase = trimmed
        .chars()
        .any(|character| character.is_ascii_uppercase());
    let has_digit = trimmed.chars().any(|character| character.is_ascii_digit());
    if whitespace_tokens.len() == 1 && (has_ascii_uppercase || has_digit) {
        return true;
    }
    let words = trimmed
        .split(|character: char| !character.is_alphanumeric() && character != '@')
        .filter(|word| word.chars().any(char::is_alphabetic))
        .collect::<Vec<_>>();
    let word_limit = if trimmed.contains('@') { 5 } else { 3 };
    !words.is_empty()
        && words.len() <= word_limit
        && words.iter().all(|word| {
            word.starts_with('@')
                || word.chars().any(char::is_numeric)
                || word
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_uppercase())
        })
}

pub(super) fn retains_source_fragment(region: &TranslationRegion) -> bool {
    if super::contract::text_is_source_equivalent(
        &region.source_text,
        &region.translated_segments.join(" "),
    ) {
        return false;
    }
    let source = comparable_characters(&region.source_text);
    let translated = comparable_characters(&region.translated_segments.join(" "));
    if source.len() < 24 || translated.len() < 24 {
        return false;
    }
    let source_windows = source.windows(3).collect::<std::collections::HashSet<_>>();
    let translated_windows = translated
        .windows(3)
        .collect::<std::collections::HashSet<_>>();
    let shared = source_windows.intersection(&translated_windows).count();
    let shorter = source_windows.len().min(translated_windows.len());
    shorter > 0 && shared * 100 >= shorter * 85
}

fn comparable_characters(text: &str) -> Vec<char> {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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
        let confident = RecognitionEvidence {
            selected_confidence: 0.95,
            ..Default::default()
        };
        assert!(!is_suspiciously_unchanged(&region("source-1"), confident));
        assert!(!is_suspiciously_unchanged(&region("名"), confident));
        assert!(!is_suspiciously_unchanged(
            &region("Example Name"),
            confident
        ));
        assert!(!is_suspiciously_unchanged(&region("0ł Noir"), confident));
        assert!(!is_suspiciously_unchanged(
            &region("#Topic#NewRelease"),
            confident
        ));
        assert!(!is_suspiciously_unchanged(
            &region("MODEL(別名)"),
            confident
        ));
        assert!(!is_suspiciously_unchanged(
            &region("Example Name (@handle): Model 2"),
            confident
        ));
        assert!(is_suspiciously_unchanged(
            &region("粉田肥門内卜用請"),
            confident
        ));
        assert!(is_suspiciously_unchanged(
            &region("This sentence was not translated"),
            confident
        ));
        assert!(!is_suspiciously_unchanged(
            &region("압그호니다"),
            RecognitionEvidence {
                selected_confidence: 0.63,
                ..Default::default()
            }
        ));
    }

    #[test]
    fn copied_source_subsets_and_supersets_require_fallback() {
        let mut subset = region("A complete source sentence with meaningful content");
        subset.translated_segments = vec!["source sentence with meaningful content".into()];
        assert!(retains_source_fragment(&subset));

        let mut translated = region("A complete source sentence with meaningful content");
        translated.translated_segments = vec!["Nội dung đã được chuyển sang đích".into()];
        assert!(!retains_source_fragment(&translated));
    }
}
