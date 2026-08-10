use reference_isolang::Language as ReferenceLanguage;
use sgt_language_catalog::Language;

#[test]
fn every_reference_language_mapping_is_preserved() {
    let mut count = 0_usize;
    while let Some(reference) = ReferenceLanguage::from_usize(count) {
        let compact = Language::from_usize(count).expect("compact catalog entry");
        assert_eq!(compact.to_639_3(), reference.to_639_3());
        assert_eq!(compact.to_639_1(), reference.to_639_1());
        assert_eq!(compact.to_name(), reference.to_name());
        assert_eq!(Language::from_639_3(reference.to_639_3()), Some(compact));
        if let Some(code) = reference.to_639_1() {
            assert_eq!(Language::from_639_1(code), Some(compact));
        }
        assert_eq!(
            Language::from_name(reference.to_name()).map(|language| language.to_639_3()),
            ReferenceLanguage::from_name(reference.to_name()).map(|language| language.to_639_3())
        );
        count += 1;
    }
    assert_eq!(count, 7_916);
    assert!(Language::from_usize(count).is_none());
}

#[test]
fn invalid_and_locale_inputs_match_the_reference_contract() {
    for value in ["", "e", "ENG", "zz", "zzz", "…"] {
        assert_eq!(
            Language::from_639_1(value).is_some(),
            ReferenceLanguage::from_639_1(value).is_some()
        );
        assert_eq!(
            Language::from_639_3(value).is_some(),
            ReferenceLanguage::from_639_3(value).is_some()
        );
    }
    for value in ["en_US.UTF-8", "de_DE", "zz_ZZ", ""] {
        assert_eq!(
            Language::from_locale(value).map(|language| language.to_639_3()),
            ReferenceLanguage::from_locale(value).map(|language| language.to_639_3())
        );
    }
}
