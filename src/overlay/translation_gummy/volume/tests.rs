use super::*;
use serde_json::{Value, json};

#[test]
fn shared_volume_transitions_retain_a_coherent_snapshot_across_view_recreation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/translation-gummy/volume-control.json"
    ))
    .unwrap();
    let default = serde_json::to_value(VolumeControl::default().snapshot()).unwrap();
    assert_eq!(
        default,
        json!({ "percent": fixture["defaultVolumePercent"], "restorePercent": fixture["defaultVolumePercent"] })
    );
    for case in fixture["cases"].as_array().unwrap() {
        let control = Arc::new(VolumeControl(Mutex::new(VolumeSnapshot {
            percent: case["initial"]["percent"].as_u64().unwrap() as u32,
            restore_percent: case["initial"]["restorePercent"].as_u64().unwrap() as u32,
        })));
        for step in case["steps"].as_array().unwrap() {
            // A new view reads the feature-owned snapshot without replacing the owner.
            let view_owner = control.clone();
            let before = control.snapshot();
            assert_eq!(view_owner.snapshot(), before);
            match step["type"].as_str().unwrap() {
                "setVolume" => view_owner.set_percent(step["value"].as_i64().unwrap()),
                "toggleMute" => view_owner.toggle_mute(),
                other => panic!("unknown fixture operation {other}"),
            }
        }
        let actual = serde_json::to_value(control.snapshot()).unwrap();
        assert_eq!(actual, case["expected"], "{}", case["name"]);
        let new_view = control.clone();
        assert_eq!(serde_json::to_value(new_view.snapshot()).unwrap(), actual);
    }
}
