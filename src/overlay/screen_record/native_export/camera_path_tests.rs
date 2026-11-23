use super::*;

// Cross-language render-math golden. The TS preview (cameraZoom.ts:
// calculateCurrentZoomStateInternal / zoomBlockEnvelope) is canonical; this
// Rust export must reproduce the SAME fixture within 1e-6 — this is the
// WYSIWYG (preview == export) lock. Regenerate via
// screen-record/tests/unit/_generateRenderGolden.gen.ts.
// See screen-record/docs/render-parity.md.
const GOLDEN: &str = include_str!("../../../../parity-fixtures/render-camera-cursor/golden.json");

fn golden() -> serde_json::Value {
    serde_json::from_str(GOLDEN).expect("golden fixture parses")
}

// ── Pure-helper behavior ──────────────────────────────────────────────────

#[test]
fn ease_camera_move_endpoints_and_midpoint() {
    // smootherStep: clamped at the ends, symmetric, 0.5 at t=0.5.
    assert_eq!(ease_camera_move(-0.5), 0.0);
    assert_eq!(ease_camera_move(0.0), 0.0);
    assert_eq!(ease_camera_move(1.0), 1.0);
    assert_eq!(ease_camera_move(1.5), 1.0);
    assert!((ease_camera_move(0.5) - 0.5).abs() < 1e-12);
    // Monotonic increasing on [0,1].
    let mut prev = ease_camera_move(0.0);
    for i in 1..=20 {
        let v = ease_camera_move(i as f64 / 20.0);
        assert!(v >= prev - 1e-12);
        prev = v;
    }
}

#[test]
fn viewport_center_round_trips() {
    // from_viewport_center(to_viewport_center(..)) is identity for zoom > 1.
    for &zoom in &[1.5_f64, 2.0, 3.7] {
        for &(px, py) in &[(0.2_f64, 0.8_f64), (0.5, 0.5), (0.9, 0.1)] {
            let (cx, cy) = to_viewport_center(zoom, px, py);
            let (rx, ry) = from_viewport_center(zoom, cx, cy);
            assert!((rx - px).abs() < 1e-9, "posX round-trip drift");
            assert!((ry - py).abs() < 1e-9, "posY round-trip drift");
        }
    }
    // At zoom <= 1 the viewport center collapses to the canvas center.
    assert_eq!(to_viewport_center(1.0, 0.2, 0.8), (0.5, 0.5));
}

#[test]
fn blend_zoom_hits_endpoints() {
    let a = ZoomState {
        zoom_factor: 1.0,
        position_x: 0.5,
        position_y: 0.5,
    };
    let b = ZoomState {
        zoom_factor: 2.5,
        position_x: 0.2,
        position_y: 0.7,
    };
    let at0 = blend_zoom(&a, &b, 0.0);
    assert!((at0.zoom_factor - a.zoom_factor).abs() < 1e-9);
    assert!((at0.position_x - a.position_x).abs() < 1e-9);
    assert!((at0.position_y - a.position_y).abs() < 1e-9);
    let at1 = blend_zoom(&a, &b, 1.0);
    assert!((at1.zoom_factor - b.zoom_factor).abs() < 1e-9);
    assert!((at1.position_x - b.position_x).abs() < 1e-9);
    assert!((at1.position_y - b.position_y).abs() < 1e-9);
}

#[test]
fn linked_manual_blocks_bypass_auto_zoom_in_gap() {
    let segment: VideoSegment = serde_json::from_value(serde_json::json!({
        "zoomBlocks": [
            {
                "startTime": 1.0, "endTime": 3.0,
                "easeIn": 0.4, "easeOut": 0.4,
                "zoomFactor": 1.5, "positionX": 0.2, "positionY": 0.2,
                "directTransitionToNext": true, "enabled": true
            },
            {
                "startTime": 6.0, "endTime": 8.0,
                "easeIn": 0.4, "easeOut": 0.4,
                "zoomFactor": 1.8, "positionX": 0.8, "positionY": 0.8,
                "enabled": true
            }
        ],
        "smoothMotionPath": [
            { "time": 0.0, "x": 500.0, "y": 500.0, "zoom": 2.0 },
            { "time": 10.0, "x": 500.0, "y": 500.0, "zoom": 2.0 }
        ]
    }))
    .expect("linked zoom segment parses");

    let state = calculate_zoom_state(4.5, &segment, 1000.0, 1000.0, (1000.0, 1000.0, 1.0));
    assert!(state.zoom_factor < (1.5_f64 * 1.8).sqrt());
    assert!(state.zoom_factor > 1.0);
    assert!((state.zoom_factor - 2.0).abs() > 0.1);
    assert!(state.position_x > 0.2 && state.position_x < 0.8);
}

#[test]
fn zoom_block_envelope_edges() {
    let b = ZoomBlock {
        start_time: 2.0,
        end_time: 8.0,
        ease_in: 1.0,
        ease_out: 1.0,
        zoom_factor: 1.5,
        position_x: 0.5,
        position_y: 0.5,
        follow_cursor: false,
        direct_transition_to_next: false,
        enabled: true,
    };
    assert_eq!(zoom_block_envelope(&b, 1.9), 0.0); // before
    assert_eq!(zoom_block_envelope(&b, 8.1), 0.0); // after
    assert_eq!(zoom_block_envelope(&b, 5.0), 1.0); // hold
    let mid_in = zoom_block_envelope(&b, 2.5);
    assert!(mid_in > 0.0 && mid_in < 1.0); // ramp-in
    let mid_out = zoom_block_envelope(&b, 7.5);
    assert!(mid_out > 0.0 && mid_out < 1.0); // ramp-out
}

// ── Cross-language golden ─────────────────────────────────────────────────

#[test]
fn camera_cases_match_golden() {
    let g = golden();
    let tol = g["tolerance"].as_f64().unwrap();
    for case in g["camera"]["cases"].as_array().unwrap() {
        let view = case["view"].as_f64().unwrap();
        let seg: VideoSegment =
            serde_json::from_value(case["segment"].clone()).expect("segment parses");
        let geometry = &case["geometry"];
        let view_w = geometry["viewW"].as_f64().unwrap_or(view);
        let view_h = geometry["viewH"].as_f64().unwrap_or(view);
        let crop_w = geometry["srcCropW"].as_f64().unwrap_or(view);
        let crop_h = geometry["srcCropH"].as_f64().unwrap_or(view);
        let scale = geometry["scale"].as_f64().unwrap_or(1.0);
        if case["name"] == "scaled_auto_landscape" {
            let canvas = (view_w, view_h, scale);
            let state = calculate_zoom_state(0.0, &seg, 800.8, 600.6, canvas);
            let frame = &generate_camera_path(&seg, 1001, 1001, 30, canvas)[0];
            assert!(((frame.x - 100.1) / 800.0 - state.position_x).abs() < tol);
            assert!(((frame.y - 200.2) / 600.0 - state.position_y).abs() < tol);
        }
        for s in case["samples"].as_array().unwrap() {
            let t = s["t"].as_f64().unwrap();
            let st = calculate_zoom_state(t, &seg, crop_w, crop_h, (view_w, view_h, scale));
            let name = case["name"].as_str().unwrap();
            assert!(
                (st.zoom_factor - s["zoom"].as_f64().unwrap()).abs() <= tol,
                "{name}: zoom drift at t={t}"
            );
            assert!(
                (st.position_x - s["posX"].as_f64().unwrap()).abs() <= tol,
                "{name}: posX drift at t={t}"
            );
            assert!(
                (st.position_y - s["posY"].as_f64().unwrap()).abs() <= tol,
                "{name}: posY drift at t={t}"
            );
        }
    }
}

#[test]
fn zoom_block_envelope_matches_golden() {
    let g = golden();
    let tol = g["tolerance"].as_f64().unwrap();
    let env = &g["camera"]["zoomBlockEnvelope"];
    let b: ZoomBlock = serde_json::from_value(env["block"].clone()).expect("envelope block parses");
    for s in env["samples"].as_array().unwrap() {
        let got = zoom_block_envelope(&b, s["t"].as_f64().unwrap());
        assert!(
            (got - s["value"].as_f64().unwrap()).abs() <= tol,
            "envelope drift at t={}",
            s["t"]
        );
    }
}
