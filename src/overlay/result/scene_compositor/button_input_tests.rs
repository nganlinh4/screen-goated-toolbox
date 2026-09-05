use super::{
    RendererInput, ResizeEdge, button_action, drag_offset, gesture_id, handle_renderer_message,
    interactive_regions, offset_from_points, resized_rect, translated_origin, update_regions,
};
use crate::overlay::result::scene_compositor::protocol::{ButtonAction, ChildEvent, SceneRect};
use windows::Win32::Foundation::{POINT, RECT};

#[test]
fn refinement_focus_messages_use_an_explicit_child_contract() {
    let cards = std::collections::HashMap::new();
    assert_eq!(
        handle_renderer_message(
            r#"{"action":"request_refine_focus","hwnd":"42"}"#,
            windows::Win32::Foundation::HWND::default(),
            &cards,
        ),
        RendererInput::FocusRefine { id: 42 }
    );
    assert_eq!(
        handle_renderer_message(
            r#"{"action":"release_refine_focus","hwnd":"0"}"#,
            windows::Win32::Foundation::HWND::default(),
            &cards,
        ),
        RendererInput::ReleaseRefineFocus
    );
}

#[test]
fn renderer_actions_become_typed_parent_events() {
    let message = serde_json::json!({ "value": 71 });
    assert_eq!(
        button_action(42, "set_opacity", &message),
        Some(ChildEvent::ButtonAction {
            id: 42,
            action: ButtonAction::SetOpacity { value: 71 }
        })
    );
    assert!(button_action(42, "unknown", &message).is_none());
    assert_eq!(
        button_action(
            42,
            "update_refine_draft",
            &serde_json::json!({ "text": "shorter" })
        ),
        Some(ChildEvent::ButtonAction {
            id: 42,
            action: ButtonAction::UpdateRefineDraft {
                text: "shorter".to_string()
            }
        })
    );
}

#[test]
fn selection_copy_is_not_a_parent_button_action() {
    let message = serde_json::json!({ "text": "selected result" });
    assert!(button_action(42, "copy_selection", &message).is_none());
}

#[test]
fn compositor_drag_offset_is_physical_and_bounded() {
    assert_eq!(
        drag_offset(&serde_json::json!({ "dx": -720, "dy": 480 })),
        (-720, 480)
    );
    assert_eq!(
        drag_offset(&serde_json::json!({ "dx": i64::MAX, "dy": i64::MIN })),
        (i32::MAX, i32::MIN)
    );
    assert_eq!(drag_offset(&serde_json::json!({})), (0, 0));
    let rect = RECT {
        left: 500,
        top: -200,
        right: 900,
        bottom: 100,
    };
    assert_eq!(translated_origin(rect, 75, -25), (575, -225));
    assert_eq!(
        translated_origin(rect, i32::MAX, i32::MIN),
        (i32::MAX, i32::MIN)
    );
    assert_eq!(
        offset_from_points(POINT { x: -100, y: 80 }, POINT { x: 50, y: -20 }),
        (150, -100)
    );
}

#[test]
fn gesture_identity_is_required_and_nonzero() {
    assert_eq!(
        gesture_id(&serde_json::json!({ "gesture_id": 42 })),
        Some(42)
    );
    assert_eq!(gesture_id(&serde_json::json!({ "gesture_id": 0 })), None);
    assert_eq!(gesture_id(&serde_json::json!({})), None);
}

#[test]
fn css_hit_regions_are_converted_once_to_physical_pixels() {
    update_regions(&serde_json::json!({
        "scale": 1.5,
        "regions": [{ "x": 10.0, "y": 20.0, "w": 30.0, "h": 40.0 }]
    }));

    assert_eq!(
        interactive_regions(),
        vec![SceneRect {
            x: 15,
            y: 30,
            width: 45,
            height: 60,
        }]
    );
    update_regions(&serde_json::json!({ "scale": 1.0, "regions": [] }));
}

#[test]
fn compositor_resize_edges_preserve_the_opposite_edge_and_native_minimum() {
    let source = include_str!("button_input.rs");
    let geometry = include_str!("button_geometry.rs");
    let rect = RECT {
        left: 100,
        top: 200,
        right: 500,
        bottom: 500,
    };
    let resized = resized_rect(rect, ResizeEdge::parse("nw").unwrap(), 80, 50);
    assert_eq!((resized.left, resized.top), (180, 250));
    assert_eq!((resized.right, resized.bottom), (500, 500));

    let minimum = resized_rect(rect, ResizeEdge::parse("se").unwrap(), -10_000, -10_000);
    assert_eq!(
        minimum.right - minimum.left,
        crate::overlay::result::event_handler::MIN_WINDOW_WIDTH
    );
    assert_eq!(
        minimum.bottom - minimum.top,
        crate::overlay::result::event_handler::MIN_WINDOW_HEIGHT
    );
    assert!(ResizeEdge::parse("center").is_none());
    assert!(source.contains("result_resize_preview"));
    assert!(geometry.contains("resized_rect(resize.start_rect"));
}
