use super::*;

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
fn virtual_desktop_clamping_is_stable() {
    let (x, y, w, h) = uia::virtual_desktop();
    let shown = clamp_to_virtual_desktop(View {
        x: x - 100,
        y: y - 50,
        w: w + 200,
        h: h + 100,
    });
    assert_eq!((shown.x, shown.y, shown.w, shown.h), (x, y, w, h));
}
