use super::*;

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
        processing_effect: Default::default(),
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
fn streaming_to_completed_transition_is_always_finalize() {
    let streaming = test_card(true);
    let completed = test_card(false);

    assert!(matches!(
        command_for_transition(Some(&streaming), &completed, "result".to_string()),
        Some(HostCommand::Finalize { .. })
    ));
}

#[test]
fn isolated_document_bridge_reports_ready_and_refits_after_resize() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("window.addEventListener('resize'"));
    assert!(bridged.contains("queueFit(window.__SGT_STREAMING__)"));
    assert!(bridged.contains("type: 'fit_request'"));
    assert!(bridged.contains("window.__SGT_FIT_CONTEXT__ = {"));
    assert!(bridged.contains("fontReady: true"));
    assert!(bridged.contains("scheduleFrame: scheduleBridgeFrame"));
    assert!(bridged.contains("cancelFrame: cancelBridgeFrame"));
    assert!(bridged.contains("type: 'frame_request'"));
    assert!(bridged.contains("commandType === 'frame_tick'"));
    assert!(bridged.contains("postToParent({ type: 'fit_complete' })"));
    assert!(bridged.contains("reportCardState('bridge_ready', null, 0"));
    assert!(bridged.contains("new MessageChannel()"));
    assert!(bridged.contains("channel.port1.addEventListener('message', handleHostMessage)"));
    assert!(bridged.contains("channel ? [channel.port2] : []"));
    assert!(bridged.contains("reportCardState('script_error'"));
    assert!(bridged.contains("querySelectorAll('script,style,template,noscript')"));
}

#[test]
fn card_content_stays_hidden_until_document_font_activation() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("if (!fontReady)"));
    assert!(bridged.contains("commandType === 'activate_font'"));
    assert!(!bridged.contains("document.fonts"));
    assert!(bridged.contains("classList.add('sgt-font-ready')"));
}

#[test]
fn card_document_waits_for_activation_before_its_first_fit() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("if (!fontReady)"));
    assert!(bridged.contains("commandType === 'activate_font'"));
    assert!(
        !bridged.contains("queueFit(window.__SGT_STREAMING__);\n  reportCardState('bridge_ready'")
    );
}

#[test]
fn isolated_document_is_authoritative_and_cannot_be_replaced_by_inner_html() {
    let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

    assert!(bridged.contains("event.data.card_id"));
    assert!(!bridged.contains("'stream_update'"));
    assert!(!bridged.contains("'finalize'"));
    assert!(!bridged.contains("document.body.innerHTML"));
    assert!(!bridged.contains("window.__SGT_APPLY_STREAM_UPDATE__"));
    assert!(!bridged.contains("fit_font_to_window_runtime"));
}
