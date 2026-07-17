use super::*;

fn element(name: &str, rect: [i32; 4], enabled: bool) -> UiElement {
    UiElement {
        name: name.to_string(),
        automation_id: format!("element-{name}"),
        runtime_id: rect.to_vec(),
        control_type: "Button",
        left: rect[0],
        top: rect[1],
        right: rect[2],
        bottom: rect[3],
        enabled,
        state: None,
        value: None,
        required: false,
    }
}

#[test]
fn empty_uia_and_small_window_chrome_are_blind() {
    let view = View {
        x: 0,
        y: 0,
        w: 1000,
        h: 800,
    };
    assert!(detector_surface_blind(&[], view));
    let chrome = vec![
        element("Minimize", [850, 0, 900, 30], true),
        element("Maximize", [900, 0, 950, 30], true),
        element("Close", [950, 0, 1000, 30], true),
    ];
    assert!(detector_surface_blind(&chrome, view));
    assert_eq!(accessible_rects(&chrome, view).len(), 3);
}

#[test]
fn accessible_form_and_disabled_noise_are_classified_correctly() {
    let view = View {
        x: 0,
        y: 0,
        w: 1000,
        h: 800,
    };
    let form = vec![
        element("Main form", [100, 100, 900, 500], true),
        element("Disabled overlay", [0, 0, 1000, 800], false),
    ];
    assert!(!detector_surface_blind(&form, view));
    assert_eq!(accessible_rects(&form, view).len(), 1);
}

#[test]
fn mutations_invalidate_anchors_but_observation_does_not() {
    for name in [
        "click_at",
        "act",
        "scroll",
        "wait",
        "browser_navigate",
        "edit_text_file",
        "edit_text_file_structure",
        "future_effect_tool",
    ] {
        assert!(action_invalidates_anchors(name), "{name}");
    }
    for name in [
        "observe",
        "look",
        "list_windows",
        "browser_read_page",
        "read_text_file",
        "map_targets",
        "click_mark",
    ] {
        assert!(!action_invalidates_anchors(name), "{name}");
    }
}

#[test]
fn anchor_view_identity_includes_position_and_size() {
    let base = View {
        x: -100,
        y: 20,
        w: 1200,
        h: 800,
    };
    assert!(same_view(base, base));
    assert!(!same_view(base, View { x: -99, ..base }));
    assert!(!same_view(base, View { w: 1199, ..base }));
}

#[test]
fn overlap_and_virtual_desktop_clamping_are_stable() {
    assert_eq!(bounds_iou([0, 0, 10, 10], [0, 0, 10, 10]), 1.0);
    assert_eq!(bounds_iou([0, 0, 10, 10], [20, 20, 30, 30]), 0.0);
    let (x, y, w, h) = uia::virtual_desktop();
    let shown = clamp_to_virtual_desktop(View {
        x: x - 100,
        y: y - 50,
        w: w + 200,
        h: h + 100,
    });
    assert_eq!((shown.x, shown.y, shown.w, shown.h), (x, y, w, h));
}

#[test]
fn detector_anchor_requires_a_nonempty_semantic_label() {
    let detected = |label: Option<&str>| super::super::super::detector::DetBox {
        cx: 20,
        cy: 20,
        score: 0.9,
        left: 10,
        top: 10,
        right: 30,
        bottom: 30,
        label: label.map(str::to_string),
    };
    assert!(labeled_detector_box(detected(None)).is_none());
    assert!(labeled_detector_box(detected(Some("   "))).is_none());
    assert_eq!(
        labeled_detector_box(detected(Some("  Save  ")))
            .unwrap()
            .label
            .as_deref(),
        Some("Save")
    );
}
