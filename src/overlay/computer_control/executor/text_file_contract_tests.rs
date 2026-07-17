use super::*;

#[test]
fn verified_large_edit_returns_bounded_full_span_content_evidence() {
    let original = format!(
        "start-marker{}middle-marker{}end-marker",
        "a".repeat(5_000),
        "b".repeat(5_000)
    );
    let fixture = Fixture::new(original.as_bytes());
    let result = edit(
        &fixture,
        json!([{"old_text": "middle-marker", "new_text": "verified-middle", "expected_count": 1}]),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert!(result["content"].is_null());
    assert_eq!(result["content_truncated"], true);
    assert!(
        result["completion_proof"]["postcondition_only"]
            .as_array()
            .unwrap()
            .contains(&json!("/content"))
    );
    assert!(
        result["completion_proof"]["partial"]
            .as_array()
            .unwrap()
            .contains(&json!("/content_sample"))
    );
    assert_eq!(result["content_sample"]["trusted_post_edit"], true);
    assert_eq!(
        result["replacement_evidence"][0]["result_exact"],
        "verified-middle"
    );
    assert_eq!(result["replacement_evidence"][0]["occurrences"], 1);
    let serialized = result["content_sample"].to_string();
    assert!(serialized.contains("start-marker"));
    assert!(serialized.contains("verified-middle"));
    assert!(serialized.contains("end-marker"));
    assert!(serialized.len() < 4 * 1024);
}

#[test]
fn every_replacement_group_has_bounded_local_post_edit_proof() {
    let original = format!(
        "head{}left-a OLD_A right-a{}left-b OLD_B right-b{}tail",
        "x".repeat(2_137),
        "y".repeat(2_291),
        "z".repeat(2_483),
    );
    let fixture = Fixture::new(original.as_bytes());
    let result = edit(
        &fixture,
        json!([
            {"old_text": "OLD_A", "new_text": "NEW_A", "expected_count": 1},
            {"old_text": "OLD_B", "new_text": "NEW_B", "expected_count": 1}
        ]),
    );
    assert_eq!(result["ok"], true, "{result}");
    let evidence = result["replacement_evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0]["replacement_index"], 0);
    assert_eq!(evidence[0]["result_exact"], "NEW_A");
    assert!(
        evidence[0]["contexts"][0]["left"]
            .as_str()
            .unwrap()
            .contains("left-a ")
    );
    assert!(
        evidence[0]["contexts"][0]["right"]
            .as_str()
            .unwrap()
            .contains(" right-a")
    );
    assert_eq!(evidence[1]["replacement_index"], 1);
    assert_eq!(evidence[1]["result_exact"], "NEW_B");
    assert!(
        evidence[1]["contexts"][0]["left"]
            .as_str()
            .unwrap()
            .contains("left-b ")
    );
    assert!(
        evidence[1]["contexts"][0]["right"]
            .as_str()
            .unwrap()
            .contains(" right-b")
    );
}

#[test]
fn long_unicode_replacement_proof_is_utf8_safe_and_bounded() {
    let fixture = Fixture::new(format!("before OLD after{}", "q".repeat(5_000)).as_bytes());
    let replacement = "β".repeat(1_000);
    let result = edit(
        &fixture,
        json!([{"old_text": "OLD", "new_text": replacement, "expected_count": 1}]),
    );
    assert_eq!(result["ok"], true, "{result}");
    let proof = &result["replacement_evidence"][0];
    assert!(proof["result_exact"].is_null());
    assert_eq!(proof["new_text_bytes"], 2_000);
    assert_eq!(proof["new_text_chars"], 1_000);
    assert_eq!(proof["result_prefix"].as_str().unwrap().chars().count(), 64);
    assert_eq!(proof["result_suffix"].as_str().unwrap().chars().count(), 64);
    assert_eq!(proof["new_text_sha256"].as_str().unwrap().len(), 64);
    assert!(proof.to_string().len() < 2 * 1024);
}
