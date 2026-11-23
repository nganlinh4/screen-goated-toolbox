use super::*;

#[test]
fn shared_exact_replacement_cases_commit_only_requested_bytes() {
    let contract: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/phone-control/file-mutation-contract.json"
    ))
    .unwrap();
    assert_eq!(contract["schemaVersion"], 6);
    for case in contract["exactReplacementCases"].as_array().unwrap() {
        let original = case["original"].as_str().unwrap();
        let fixture = Fixture::named(case["path"].as_str().unwrap(), original.as_bytes());
        let result = edit(&fixture, case["replacements"].clone());
        let code = case["resultCode"].as_str().unwrap();
        if code == "ok" {
            assert_eq!(result["ok"], true, "{}: {result}", case["name"]);
            assert_eq!(
                fs::read(&fixture.path).unwrap(),
                case["output"].as_str().unwrap().as_bytes()
            );
            assert_eq!(result["effect_verified"], true);
        } else {
            assert_eq!(result["code"], code, "{}: {result}", case["name"]);
            assert_eq!(result["effect_may_have_occurred"], false);
            assert_eq!(fs::read(&fixture.path).unwrap(), original.as_bytes());
        }
    }
}

#[test]
fn shared_replacement_resource_limits_reject_without_writing() {
    let contract: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/phone-control/file-mutation-contract.json"
    ))
    .unwrap();
    assert_eq!(
        contract["invariants"]["maxTotalReplacementOccurrences"],
        MAX_REPLACEMENT_MATCHES
    );
    for case in contract["resourceCases"].as_array().unwrap() {
        let count = case["repeatCount"].as_u64().unwrap() as usize;
        let original = case["originalUnit"].as_str().unwrap().repeat(count);
        let fixture = Fixture::new(original.as_bytes());
        let result = edit(
            &fixture,
            json!([{
                "old_text": case["originalUnit"], "new_text": case["replacementUnit"], "expected_count": count
            }]),
        );
        assert_eq!(
            result["code"], case["resultCode"],
            "{}: {result}",
            case["name"]
        );
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(fs::read(&fixture.path).unwrap(), original.as_bytes());
    }
}

#[test]
fn output_size_limit_accounts_for_original_bom_before_allocating_output() {
    let mut original = UTF8_BOM.to_vec();
    original.push(b'a');
    let fixture = Fixture::new(&original);
    let replacement = "x".repeat(MAX_FILE_BYTES as usize - UTF8_BOM.len() + 1);
    let result = edit(
        &fixture,
        json!([{"old_text":"a", "new_text": replacement, "expected_count":1}]),
    );
    assert_eq!(result["code"], "ERR_TEXT_FILE_TOO_LARGE", "{result}");
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn malformed_surrogate_json_is_rejected_before_dispatch() {
    let contract: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/phone-control/file-mutation-contract.json"
    ))
    .unwrap();
    for case in contract["encodingCases"].as_array().unwrap() {
        let units = case["utf16Units"].as_array().unwrap();
        let escaped = units
            .iter()
            .map(|unit| format!("\\u{:04X}", unit.as_u64().unwrap()))
            .collect::<String>();
        assert_eq!(
            serde_json::from_str::<String>(&format!("\"{escaped}\"")).is_err(),
            case["windowsJsonRejected"].as_bool().unwrap()
        );
    }
}
