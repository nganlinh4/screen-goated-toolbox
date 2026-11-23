use super::*;

#[test]
fn shared_query_terms_count_unicode_scalars() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/public-research-relevance.json"
    )))
    .unwrap();
    for case in fixture["token_cases"].as_array().unwrap() {
        let expected = case["terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|term| term.as_str().unwrap().to_string())
            .collect::<HashSet<_>>();
        assert_eq!(
            tokens(case["query"].as_str().unwrap()),
            expected,
            "{}",
            case["name"]
        );
    }
}

#[test]
fn shared_short_terms_match_only_at_lexical_starts() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/public-research-relevance.json"
    )))
    .unwrap();
    for case in fixture["match_cases"].as_array().unwrap() {
        let term = case["term"].as_str().unwrap();
        let text = case["text"].as_str().unwrap().to_lowercase();
        assert_eq!(
            match_offset(term, &text).is_some(),
            case["matches"].as_bool().unwrap(),
            "{case}"
        );
    }
}
