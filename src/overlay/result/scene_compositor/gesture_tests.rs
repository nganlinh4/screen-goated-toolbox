use super::*;
use std::collections::HashMap;

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_card(id: isize, rect: SceneRect) -> SceneCard {
    SceneCard {
        id,
        rect: rect.clone(),
        control_rect: rect,
        body: "result".to_string(),
        document: Some("test".to_string()),
        external_navigation: false,
        navigation_loading: false,
        refining: false,
        background: "#000".to_string(),
        opacity: 100,
        visible: true,
        streaming: false,
        streaming_enabled: false,
        stack_order: 1,
        controls: Default::default(),
        presentation: crate::overlay::result::ResultPresentation::Standard,
        backdrop_data_url: None,
        foreground_color: None,
        preferred_font_size: None,
        source_vertical: false,
        source_regions: Vec::new(),
        source_segments: Vec::new(),
        source_replacement: false,
    }
}

#[test]
fn gesture_starts_inactive() {
    let _guard = TEST_LOCK.lock().unwrap();
    assert!(!is_gesture_active());
    assert_eq!(active_gesture_id(), None);
}

#[test]
fn stale_finish_does_not_destroy_newer_gesture() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut cards = HashMap::new();
    cards.insert(
        -42,
        test_card(
            -42,
            SceneRect {
                x: 10,
                y: 10,
                width: 200,
                height: 200,
            },
        ),
    );

    let event1 = begin_drag(
        -42,
        DragMode::One,
        100,
        1,
        POINT::default(),
        HWND::default(),
        &cards,
    );
    assert!(event1.is_some());
    assert!(is_gesture_active());
    assert_eq!(active_gesture_id(), Some(100));

    // Start gesture 200 (supersedes 100)
    let event2 = begin_drag(
        -42,
        DragMode::One,
        200,
        1,
        POINT::default(),
        HWND::default(),
        &cards,
    );
    assert!(event2.is_some());
    assert_eq!(active_gesture_id(), Some(200));

    // Delivery of stale finish for gesture 100 must be rejected and must NOT destroy gesture 200
    let stale_result = finish_gesture_with_offset(100, 50, 50);
    assert_eq!(stale_result, None);
    assert!(is_gesture_active());
    assert_eq!(active_gesture_id(), Some(200));

    preview_gesture(200, 300, 50);
    let first = visual_preview_rects(&cards);
    assert_eq!(first[0].x, 310);
    preview_gesture(200, -100, 50);
    let swept = visual_preview_rects(&cards);
    assert_eq!((swept[0].x, swept[0].width), (-90, 600));

    // Delivery of valid finish for gesture 200 succeeds
    let valid_result = finish_gesture_with_offset(200, 50, 50);
    assert!(valid_result.is_some());
    assert!(!is_gesture_active());
    assert_eq!(active_gesture_id(), None);
    assert!(!visual_preview_rects(&cards).is_empty());
    assert!(super::super::button_input::settle_drag(Some(200)));
    assert!(visual_preview_rects(&cards).is_empty());
}

#[test]
fn card_removal_cancels_active_gesture_with_cancelled_outcome() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut cards = HashMap::new();
    cards.insert(
        -77,
        test_card(
            -77,
            SceneRect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
        ),
    );

    let _ = begin_drag(
        -77,
        DragMode::One,
        301,
        1,
        POINT::default(),
        HWND::default(),
        &cards,
    );
    assert_eq!(active_gesture_id(), Some(301));

    cancel_removed_card(-77);
    assert!(!is_gesture_active());
}

#[test]
fn out_of_bounds_drag_and_finish_completes_cleanly() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut cards = HashMap::new();
    cards.insert(
        -88,
        test_card(
            -88,
            SceneRect {
                x: 50,
                y: 50,
                width: 200,
                height: 200,
            },
        ),
    );

    let _ = begin_drag(
        -88,
        DragMode::One,
        401,
        1,
        POINT { x: -100, y: 80 },
        HWND::default(),
        &cards,
    );
    assert!(is_gesture_active());

    // Preview large out-of-bounds offset
    preview_gesture(401, 5000, -3000);

    // Native release uses the original press even after asynchronous recognition.
    observe_mouse_event(WM_LBUTTONUP, POINT { x: 4900, y: -2920 });
    assert_eq!(
        PENDING_FINISH.lock().unwrap().take(),
        Some((401, 5000, -3000))
    );

    let finished = finish_gesture_with_offset(401, 5000, -3000);
    assert!(finished.is_some());
    assert!(!is_gesture_active());
    assert_eq!(active_gesture_id(), None);
}
