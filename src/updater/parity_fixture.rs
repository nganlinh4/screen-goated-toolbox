use serde_json::Value;

pub(super) fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../parity-fixtures/app-update/latest-release.json"
    ))
    .unwrap()
}

pub(super) fn payload(fixture: &Value, case: &Value) -> Value {
    let key = match case["source"].as_str().unwrap() {
        "manifest" => "stable_manifest",
        "fallback" => "latest_release",
        _ => panic!("unknown update fixture source"),
    };
    let mut payload = fixture[key].clone();
    for mutation in case["mutations"].as_array().unwrap() {
        let pointer = mutation["pointer"].as_str().unwrap();
        match mutation["operation"].as_str().unwrap_or("replace") {
            "replace" => *payload.pointer_mut(pointer).unwrap() = mutation["value"].clone(),
            "remove" => {
                let (parent, name) = pointer.rsplit_once('/').unwrap();
                assert!(
                    payload
                        .pointer_mut(parent)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove(name)
                        .is_some()
                );
            }
            "duplicate" => {
                let value = payload.pointer(pointer).unwrap().clone();
                let (parent, _) = pointer.rsplit_once('/').unwrap();
                payload
                    .pointer_mut(parent)
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .push(value);
            }
            _ => panic!("unknown update fixture mutation"),
        }
    }
    payload
}

#[test]
fn shared_update_applicability_uses_canonical_versions() {
    let fixture = fixture();
    let remote =
        semver::Version::parse(fixture["stable_manifest"]["version"].as_str().unwrap()).unwrap();
    for case in fixture["comparison_examples"].as_array().unwrap() {
        let newer =
            super::bump_is_greater(case["canonical_version"].as_str().unwrap(), &remote).unwrap();
        assert_eq!(newer, case["expected_status"] == "update_available");
    }
}
