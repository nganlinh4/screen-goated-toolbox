use super::*;

#[test]
fn shared_short_queries_select_bounded_matching_passages() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/public-research-relevance.json"
    )))
    .unwrap();
    for case in fixture["excerpt_cases"].as_array().unwrap() {
        let before = case["filler_before"].as_u64().unwrap() as usize;
        let after = case["filler_after"].as_u64().unwrap() as usize;
        let line = case["line"].as_str().unwrap();
        let max_chars = case["max_chars"].as_u64().unwrap() as usize;
        let body = format!(
            "{}{line}\n{}",
            "neutral passage context\n".repeat(before),
            "neutral passage context\n".repeat(after)
        );
        let excerpt = relevant_body_sample(&body, max_chars, case["query"].as_str().unwrap());
        assert!(excerpt.contains(line), "{}: {excerpt}", case["name"]);
        assert!(excerpt.chars().count() <= max_chars);
    }
}

#[test]
fn short_and_no_match_budgets_remain_bounded() {
    let body = "neutral content\n".repeat(100);
    for limit in [0, 1, 12, 80] {
        assert!(relevant_body_sample(&body, limit, "x").chars().count() <= limit);
        assert!(
            relevant_body_sample(&body, limit, "unmatched")
                .chars()
                .count()
                <= limit
        );
    }
}

#[test]
fn shared_scalar_budget_and_long_line_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/public-research-relevance.json"
    )))
    .unwrap();
    for case in fixture["scalar_budget_cases"].as_array().unwrap() {
        let actual = relevant_body_sample(
            case["text"].as_str().unwrap(),
            case["max_chars"].as_u64().unwrap() as usize,
            case["query"].as_str().unwrap(),
        );
        assert_eq!(actual, case["expected"].as_str().unwrap(), "{case}");
    }
    for case in fixture["long_line_cases"].as_array().unwrap() {
        let line = format!(
            "{}{}{}",
            case["prefix"]
                .as_str()
                .unwrap()
                .repeat(case["prefix_repetitions"].as_u64().unwrap() as usize),
            case["match"].as_str().unwrap(),
            case["suffix"].as_str().unwrap()
        );
        let limit = case["max_chars"].as_u64().unwrap() as usize;
        let actual = relevant_body_sample(&line, limit, case["query"].as_str().unwrap());
        assert!(actual.contains(case["match"].as_str().unwrap()), "{actual}");
        assert!(actual.chars().count() <= limit);
    }
    for case in fixture["ranking_cases"].as_array().unwrap() {
        let distractor = format!("{}\n", case["distractor"].as_str().unwrap());
        let expected = case["match"].as_str().unwrap();
        let body = format!(
            "{}{expected}\n{}",
            distractor.repeat(case["distractor_repetitions"].as_u64().unwrap() as usize),
            distractor.repeat(40)
        );
        let actual = relevant_body_sample(
            &body,
            case["max_chars"].as_u64().unwrap() as usize,
            case["query"].as_str().unwrap(),
        );
        assert!(actual.contains(expected), "{actual}");
    }
}
