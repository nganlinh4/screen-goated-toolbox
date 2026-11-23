use super::unique_option_index;
use serde_json::json;

#[test]
fn select_does_not_confuse_one_with_ten() {
    let options = vec![
        json!({"index": 0, "text": "10", "value": "10"}),
        json!({"index": 1, "text": "1", "value": "1"}),
    ];
    assert_eq!(unique_option_index(&options, "1").unwrap(), 1);
}

#[test]
fn select_requires_a_unique_exact_normalized_option() {
    let options = vec![
        json!({"index": 0, "text": " Standard ", "value": "a"}),
        json!({"index": 1, "text": "standard", "value": "b"}),
    ];
    let error = unique_option_index(&options, "STANDARD").unwrap_err();
    assert!(error.to_string().contains("ambiguous option"));
}
