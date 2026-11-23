use super::*;

#[test]
fn native_geometry_retains_absolute_screen_coordinates_and_insets() {
    let bounds = RECT {
        left: -1600,
        top: -280,
        right: -800,
        bottom: 320,
    };
    let standard = geometry_from_screen(7, bounds, true, ResultPresentation::Standard);
    assert_eq!((standard.rect.x, standard.rect.y), (-1596, -278));
    assert_eq!(
        (standard.control_rect.x, standard.control_rect.y),
        (-1600, -280)
    );
    assert_eq!((standard.rect.width, standard.rect.height), (792, 596));
    let text = geometry_from_screen(7, bounds, false, ResultPresentation::TextOnly);
    assert_eq!((text.rect.x, text.rect.y), (-1600, -280));
    assert!(!text.visible);
}

#[test]
fn onboarding_control_pulse_is_reserved_exactly_ten_times() {
    let mut count = 0;
    for expected in 1..=RESULT_CONTROLS_ONBOARDING_LIMIT {
        assert_eq!(
            reserve_result_controls_onboarding_pulse(&mut count),
            Some(expected)
        );
        assert_eq!(count, expected);
    }
    assert_eq!(reserve_result_controls_onboarding_pulse(&mut count), None);
    assert_eq!(count, RESULT_CONTROLS_ONBOARDING_LIMIT);
}

fn test_card(streaming: bool) -> SceneCard {
    SceneCard {
        id: 42,
        rect: SceneRect {
            x: 1,
            y: 2,
            width: 300,
            height: 100,
        },
        control_rect: SceneRect {
            x: 0,
            y: 0,
            width: 308,
            height: 104,
        },
        body: "result".to_string(),
        document: None,
        external_navigation: false,
        navigation_loading: false,
        refining: false,
        background: "#ffffff".to_string(),
        opacity: 90,
        visible: true,
        streaming,
        streaming_enabled: streaming,
        stack_order: 7,
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
fn identical_completed_sync_is_not_dispatched_again() {
    let card = test_card(false);
    assert_eq!(
        command_for_transition(Some(&card), &card, "result".to_string()),
        None
    );
}

#[test]
fn compositor_owned_groups_ignore_hidden_controller_window_geometry() {
    let source = include_str!("parent.rs");
    let sync = source
        .split("pub fn sync_geometry")
        .nth(1)
        .unwrap()
        .split("fn settle_drag_geometry")
        .next()
        .unwrap();

    assert!(sync.contains("scene_groups::owns_geometry"));
    assert!(sync.find("scene_groups::owns_geometry").unwrap() < sync.find("IsWindow").unwrap());
}

#[test]
fn streaming_to_completed_transition_is_always_finalize() {
    let streaming = test_card(true);
    let completed = test_card(false);

    assert!(matches!(
        command_for_transition(Some(&streaming), &completed, "result".to_string()),
        Some(HostCommand::Finalize { .. })
    ));
}

// The isolated-document bridge (handshake, font gating, fit and frame relay,
// identity checks, diagnostics) runs in card_bridge_runtime.test.cjs.
