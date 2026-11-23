//! Target-language picker names and their resolution to one ISO 639 entry.
//!
//! The pickers list the English name of every language that has an ISO 639-1
//! code. Several ISO 639-3 rows share an English name, and the catalog's own
//! name lookup returns the first row, which can be a language without a 639-1
//! code. A picker name therefore resolves to the listed 639-1 language first.

use std::collections::HashMap;
use std::sync::LazyLock;

fn picker_catalog() -> impl Iterator<Item = isolang::Language> {
    (0..)
        .map_while(isolang::Language::from_usize)
        .filter(|language| language.to_639_1().is_some())
}

static PICKER_LANGUAGES_BY_NAME: LazyLock<HashMap<&'static str, isolang::Language>> =
    LazyLock::new(|| {
        let mut by_name = HashMap::new();
        for language in picker_catalog() {
            by_name.entry(language.to_name()).or_insert(language);
        }
        by_name
    });

/// Every picker language name, sorted and deduplicated.
#[cfg(not(feature = "recorder-worker"))]
static ALL_LANGUAGES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut languages: Vec<String> = PICKER_LANGUAGES_BY_NAME
        .keys()
        .map(|name| name.to_string())
        .collect();
    languages.sort();
    languages
});

/// Get all available language names
#[cfg(not(feature = "recorder-worker"))]
pub fn get_all_languages() -> &'static Vec<String> {
    &ALL_LANGUAGES
}

/// The language an English name means: the picker's ISO 639-1 language when
/// one has that name, otherwise the catalog's first row with it.
pub fn language_from_name(name: &str) -> Option<isolang::Language> {
    let name = name.trim();
    PICKER_LANGUAGES_BY_NAME
        .get(name)
        .copied()
        .or_else(|| isolang::Language::from_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picker_name_resolves_to_the_listed_two_letter_language() {
        for language in picker_catalog() {
            let resolved = language_from_name(language.to_name()).expect("picker name resolves");
            assert!(
                resolved.to_639_1().is_some(),
                "{} resolved to {}",
                language.to_name(),
                resolved.to_639_3()
            );
        }
    }

    #[test]
    fn a_name_shared_with_an_earlier_catalog_row_resolves_to_the_picker_language() {
        let shared: Vec<isolang::Language> = picker_catalog()
            .filter(|language| isolang::Language::from_name(language.to_name()) != Some(*language))
            .collect();
        assert!(!shared.is_empty(), "the catalog has shared English names");
        for language in shared {
            assert_eq!(language_from_name(language.to_name()), Some(language));
        }
    }

    #[test]
    fn names_outside_the_picker_still_resolve_through_the_catalog() {
        let language = (0..)
            .map_while(isolang::Language::from_usize)
            .find(|language| {
                language.to_639_1().is_none()
                    && !PICKER_LANGUAGES_BY_NAME.contains_key(language.to_name())
            })
            .expect("a language without a two-letter code");
        assert_eq!(
            language_from_name(language.to_name()),
            isolang::Language::from_name(language.to_name())
        );
        assert_eq!(language_from_name("  "), None);
    }
}
