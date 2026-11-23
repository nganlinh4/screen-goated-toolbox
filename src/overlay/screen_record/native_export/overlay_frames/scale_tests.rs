//! An export at another resolution than the preview must draw the keystroke
//! lane and text animations at the same size relative to the frame.

use super::*;

/// Overlay metadata as the TS bake sends it, with every pixel length
/// authored at `scale` times the preview's.
fn metadata(scale: f64) -> OverlayAtlasMetadata {
    let px = |value: f64| value * scale;
    let keystroke = |key: &str| {
        serde_json::json!({
            "uniqueKey": key,
            "normalRectX": 0.0, "normalRectY": 0.0,
            "normalRectW": px(48.0), "normalRectH": px(22.0),
            "heldRectX": 0.0, "heldRectY": px(22.0),
            "heldRectW": px(48.0), "heldRectH": px(22.0),
            // A small font, so the fixed gaps and margins are what apply.
            "layoutWidth": px(40.0), "layoutHeight": px(14.0),
            "layoutFontSize": px(10.0), "layoutMarginBottom": px(14.0),
            "pad": px(4.0), "bubbleWidth": px(48.0),
        })
    };
    let event = |key: &str, kind: &str, start: f64| {
        serde_json::json!({
            "uniqueKey": key, "type": kind,
            "startTime": start, "endTime": 2.0, "isHold": false,
        })
    };
    serde_json::from_value(serde_json::json!({
        "atlasWidth": px(256.0) as u32, "atlasHeight": px(256.0) as u32,
        "textEntries": [{
            "startTime": 0.0, "endTime": 1.0,
            "rectX": 0.0, "rectY": px(100.0), "rectW": px(60.0), "rectH": px(20.0),
            "hitX": px(100.0), "hitY": px(100.0),
            "pivotX": px(130.0), "pivotY": px(110.0), "pad": px(4.0),
            "animationPreset": "slide-up",
            "animationInDuration": 0.3, "animationOutDuration": 0.3,
        }],
        "keystrokeEntries": [keystroke("a"), keystroke("b"), keystroke("c"), keystroke("m")],
        "keystrokeMode": "keyboardMouse",
        "overlayX": 50.0, "overlayY": 100.0, "overlayScale": 1.0,
        "visibilitySegments": [{ "startTime": 0.0, "endTime": 2.0 }],
        "displayEvents": [
            event("a", "keydown", 0.0),
            event("m", "mousedown", 0.02),
            event("b", "keydown", 0.05),
            event("c", "keydown", 0.10),
        ],
        "keyboardStartTimes": [0.0, 0.05, 0.10],
        "keyboardIndices": [0, 2, 3],
        "mouseStartTimes": [0.02],
        "mouseIndices": [1],
        "keyboardMaxDuration": 2.0,
        "mouseMaxDuration": 2.0,
        "eventSlots": [2, 0, 1, 0],
        "eventIdentities": ["a", "m", "b", "c"],
        "keyboardSlotRepresentativeWidths": [px(48.0), px(48.0), px(48.0)],
        "mouseSlotRepresentativeWidths": [px(48.0)],
        "renderScale": scale,
    }))
    .unwrap()
}

fn frames(scale: f64) -> Vec<OverlayFrame> {
    let trim: Vec<TrimSegment> =
        serde_json::from_value(serde_json::json!([{ "startTime": 0.0, "endTime": 1.0 }])).unwrap();
    generate_overlay_frames(
        &metadata(scale),
        &trim,
        &[],
        30,
        1280.0 * scale,
        720.0 * scale,
    )
}

#[test]
fn a_2x_export_draws_every_overlay_quad_at_twice_the_preview_geometry() {
    let preview = frames(1.0);
    let export = frames(2.0);
    assert_eq!(preview.len(), export.len());
    // Frames inside the enter animations, during the hold, and late on.
    for index in [2, 4, 8, 20] {
        let (small, large) = (&preview[index].quads, &export[index].quads);
        assert!(small.len() >= 4, "frame {index} shows text and keystrokes");
        assert_eq!(small.len(), large.len(), "frame {index}");
        for (quad, (a, b)) in small.iter().zip(large).enumerate() {
            for (name, preview_px, export_px) in [
                ("x", a.x, b.x),
                ("y", a.y, b.y),
                ("w", a.w, b.w),
                ("h", a.h, b.h),
            ] {
                assert!(
                    (export_px - preview_px * 2.0).abs() <= 2.0,
                    "frame {index} quad {quad} {name}: preview {preview_px} export {export_px}"
                );
            }
            assert!((a.alpha - b.alpha).abs() < 1e-4);
        }
    }
}

#[test]
fn a_payload_without_a_usable_render_scale_renders_at_preview_size() {
    for value in [
        serde_json::Value::Null,
        serde_json::json!(0.0),
        serde_json::json!(-2.0),
    ] {
        let mut raw = serde_json::json!({ "atlasWidth": 1, "atlasHeight": 1 });
        if !value.is_null() {
            raw["renderScale"] = value;
        }
        let meta: OverlayAtlasMetadata = serde_json::from_value(raw).unwrap();
        assert_eq!(meta.effective_render_scale(), 1.0);
    }
}
