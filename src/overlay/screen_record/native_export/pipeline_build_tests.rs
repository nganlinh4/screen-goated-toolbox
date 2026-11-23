use super::super::pipeline::camera_source;
use super::{ExportConfig, UniformBuildParams, background_presets};

#[test]
fn scaled_follow_to_fixed_transition_matches_preview_bounds_between_frames() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/render-camera-cursor/golden.json"
    ))
    .unwrap();
    let case = fixture["camera"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "scaled_follow_to_fixed_transition")
        .unwrap();
    let config: ExportConfig = serde_json::from_value(serde_json::json!({
        "width": 1600, "height": 900, "framerate": 4,
        "trimStart": 0, "duration": 10, "segment": case["segment"],
        "backgroundConfig": {
            "scale": 90, "borderRadius": 0, "backgroundType": "solid",
            "shadow": 0, "cursorScale": 1
        }
    }))
    .unwrap();
    let params = UniformBuildParams {
        // The production selector: the worker derives this camera itself.
        camera: camera_source(Vec::new(), &config, 1000, 1000, 1600, 900),
        parsed_baked_cursor: vec![],
        capture_dimensions: Default::default(),
        src_w: 1000,
        src_h: 1000,
        crop_w: 800,
        crop_h: 600,
        camera_crop_w: 800,
        camera_crop_h: 600,
        crop_x_offset: 100.0,
        crop_y_offset: 200.0,
        out_w: 1600,
        out_h: 900,
        out_aspect: 1600.0 / 900.0,
        scale_factor: 0.9,
        cursor_scale_cfg: 1.0,
        cursor_shadow: 0.0,
        use_custom_background: false,
        actual_bg_w: 0.0,
        actual_bg_h: 0.0,
        shadow_opacity: 0.0,
        shadow_blur: 0.0,
        shadow_offset: 0.0,
        border_radius: 0.0,
        background_preset: background_presets::BuiltInBackgroundPreset::default(),
        bg_style: 0.0,
    };
    let tolerance = fixture["tolerance"].as_f64().unwrap();
    for sample in case["samples"].as_array().unwrap() {
        let t = sample["t"].as_f64().unwrap();
        let uniforms = params.build_uniforms(&config, t, t, t, t);
        let bounds = [
            uniforms.video_offset[0],
            uniforms.video_offset[1],
            uniforms.video_scale[0],
            uniforms.video_scale[1],
        ];
        for (axis, actual) in bounds.iter().enumerate() {
            let expected = sample["bounds"][axis].as_f64().unwrap();
            assert!(
                (f64::from(*actual) - expected).abs() < tolerance,
                "t={t} bounds[{axis}]: actual={actual} expected={expected}"
            );
        }
    }
}
