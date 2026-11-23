//! Replays `retiredModes` from `parity-fixtures/image-to-3d/input-contract.json`:
//! the host refuses to validate a source for a retired generation mode, so the
//! page's preflight fails closed with the fixture's error.

use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn retired_generation_modes_are_refused_by_source_validation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/image-to-3d/input-contract.json"
    ))
    .unwrap();
    let root = std::env::temp_dir().join(format!("sgt-3d-retired-mode-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (index, case) in fixture["retiredModes"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let path = root.join(format!("retired-{index}.png"));
        image::RgbImage::new(
            case["width"].as_u64().unwrap() as u32,
            case["height"].as_u64().unwrap() as u32,
        )
        .save(&path)
        .unwrap();
        // The same readable source is accepted for Quality, so the refusal
        // below comes from the retired mode, not from the image.
        let quality = json!({"path": path, "generationMode": "quality"});
        assert_eq!(
            dispatch("validate_image", &quality, &AtomicBool::new(true)).unwrap()["error"],
            Value::Null
        );
        let retired = json!({"path": path, "generationMode": case["mode"]});
        assert!(
            dispatch("validate_image", &retired, &AtomicBool::new(true)).is_err(),
            "{case}"
        );
        // The page maps a refused validation to this public error code.
        assert_eq!(case["error"], "validation_unavailable");
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::remove_dir(&root).unwrap();
}
