use super::*;
use crate::updater::parity_fixture::{fixture, payload};

#[test]
fn shared_update_manifest_contract_cases() {
    let fixture = fixture();
    for case in fixture["acceptance_cases"].as_array().unwrap() {
        if case["source"] != "manifest" {
            continue;
        }
        let result = parse(&serde_json::to_vec(&payload(&fixture, case)).unwrap());
        assert_eq!(
            result.is_ok(),
            case["windows"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if let Ok(candidate) = result {
            assert_eq!(candidate.version.to_string(), "9.9.9");
            assert_eq!(candidate.asset_name, "ScreenGoatedToolbox_v9.9.9.exe");
            assert_eq!(candidate.size_bytes, 123_456_789);
            assert_eq!(candidate.sha256, "1".repeat(64));
            assert_eq!(
                candidate.body,
                case["body"].as_str().unwrap_or("- Synthetic release note")
            );
        }
    }
}
