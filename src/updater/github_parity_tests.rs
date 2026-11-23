use super::*;
use crate::updater::parity_fixture::{fixture, payload};

#[test]
fn shared_update_fallback_contract_cases() {
    let fixture = fixture();
    for case in fixture["acceptance_cases"].as_array().unwrap() {
        if case["source"] != "fallback" {
            continue;
        }
        let result = serde_json::from_value::<GithubRelease>(payload(&fixture, case))
            .ok()
            .and_then(candidate_from_release);
        assert_eq!(
            result.is_some(),
            case["windows"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if let Some(candidate) = result {
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
