use super::*;

#[test]
fn empty_cards_with_stale_regions_hides_window_and_returns_hidden() {
    let _lock = REGION_LOCK.lock().unwrap();
    let hwnd = create_input_surface(0, 0, 800, 600).expect("create test surface");
    assert_eq!(unsafe { GetPropW(hwnd, w!("NonRudeHWND")) }.0 as usize, 1);
    let cards: HashMap<isize, SceneCard> = HashMap::new();
    let stale_regions = vec![SceneRect {
        x: 10,
        y: 10,
        width: 100,
        height: 100,
    }];

    let state = update_input_regions(hwnd, &cards, &stale_regions, 0, 0, 800, 600);
    assert_eq!(state.visible_cards, 0);
    assert_eq!(state.input_region_count, 0);
    assert!(state.is_hidden);

    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

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
        background: "#ffffff".to_string(),
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
fn input_surface_region_aligns_on_negative_origin_desktop() {
    let _lock = REGION_LOCK.lock().unwrap();
    let hwnd = create_input_surface(-1920, 0, 3840, 1080).expect("create test surface");
    let mut cards = HashMap::new();
    cards.insert(
        1,
        test_card(
            1,
            SceneRect {
                x: 500,
                y: 100,
                width: 400,
                height: 300,
            },
        ),
    );

    let state = update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080);
    assert_eq!(state.visible_cards, 1);
    assert_eq!(state.input_region_count, 1);
    assert!(!state.is_hidden);

    unsafe {
        assert_eq!(
            update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080),
            state
        );
        let _ = ShowWindow(hwnd, SW_HIDE);
        assert_eq!(
            update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080),
            state
        );
        assert!(IsWindowVisible(hwnd).as_bool());
        cards.get_mut(&1).unwrap().rect.x = 900;
        let _ = update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080);
        let actual = CreateRectRgn(0, 0, 0, 0);
        let expected = CreateRectRgn(900, 100, 1300, 400);
        assert_ne!(GetWindowRgn(hwnd, actual).0, 0);
        assert!(EqualRgn(actual, expected).as_bool());
        let _ = DeleteObject(actual.into());
        let _ = DeleteObject(expected.into());
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
fn passive_results_never_own_input_even_with_stale_control_regions() {
    let _lock = REGION_LOCK.lock().unwrap();
    let hwnd = create_input_surface(0, 0, 800, 600).expect("create test surface");
    let rect = SceneRect {
        x: 50,
        y: 50,
        width: 200,
        height: 100,
    };
    let mut passive = test_card(1, rect.clone());
    passive.controls.input_passthrough = true;
    let mut cards = HashMap::from([(1, passive)]);
    let state = update_input_regions(hwnd, &cards, &[rect], 0, 0, 800, 600);
    assert!(state.is_hidden);
    assert_eq!(state.input_region_count, 0);
    assert!(!state.blocks_visuals(&cards));
    cards.insert(
        2,
        test_card(
            2,
            SceneRect {
                x: 400,
                y: 100,
                width: 100,
                height: 100,
            },
        ),
    );
    let state = update_input_regions(hwnd, &cards, &[], 0, 0, 800, 600);
    assert!(!state.is_hidden);
    assert_eq!(state.input_region_count, 1);
    unsafe {
        let actual = CreateRectRgn(0, 0, 0, 0);
        let expected = CreateRectRgn(400, 100, 500, 200);
        assert_ne!(GetWindowRgn(hwnd, actual).0, 0);
        assert!(EqualRgn(actual, expected).as_bool());
        let _ = DeleteObject(actual.into());
        let _ = DeleteObject(expected.into());
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
fn fault_injected_region_failure_immediately_hides_surface() {
    let _lock = REGION_LOCK.lock().unwrap();
    let hwnd = create_input_surface(0, 0, 800, 600).expect("create test surface");
    let mut cards = HashMap::new();
    cards.insert(
        1,
        test_card(
            1,
            SceneRect {
                x: 50,
                y: 50,
                width: 200,
                height: 200,
            },
        ),
    );

    INJECT_REGION_FAILURE.store(true, Ordering::SeqCst);
    let state = update_input_regions(hwnd, &cards, &[], 0, 0, 800, 600);
    INJECT_REGION_FAILURE.store(false, Ordering::SeqCst);

    assert!(state.is_hidden);
    assert_eq!(state.visible_cards, 0);

    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
