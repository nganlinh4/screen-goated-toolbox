use super::*;

fn browser_window() -> super::super::controller::world::BrowserWindowIdentity {
    super::super::controller::world::BrowserWindowIdentity {
        browser_window_id: 2,
        hwnd: 3,
        pid: 4,
        generation: 5,
    }
}

#[test]
fn new_turn_clears_task_local_recovery_state() {
    let mut brain = Brain::new(None);
    brain.begin_job(11, None, false);
    brain.trail.push("observe=ok".to_string());
    brain.recent_actions.push("observe|{}".to_string());
    brain.advice_latches.push("observe|state".to_string());
    brain.prev_state_sig = Some("old surface".to_string());
    brain.wait_accum = 5.0;
    brain.last_click = Some((10, 20));
    brain.click_before = Some(vec![1, 2, 3]);
    brain.active_action = Some(super::super::telemetry::ActionTrace {
        action_id: 9,
        turn_id: 11,
    });
    brain.controlled_tab_id = Some(73);
    brain.controlled_document_id = Some("document-before".into());
    brain.next_anchor_id = 42;
    brain.zoomed = true;
    brain.whole_screen = true;
    brain.show_coarse_grid = true;
    brain.view = View {
        x: -10,
        y: -20,
        w: 30,
        h: 40,
    };
    brain.anchors.push(ClickAnchor {
        id: 41,
        x: 10,
        y: 20,
        note: None,
        verify_description: None,
        source: AnchorSource::Detector,
        score: None,
        bounds: None,
        frame_id: 1,
        view: brain.view,
        surface: super::super::controller::world::SurfaceIdentity::Native {
            hwnd: 1,
            pid: 1,
            generation: 1,
        },
    });

    brain.begin_job(11, None, false);
    assert_eq!(brain.trail, ["observe=ok"]);
    assert_eq!(brain.anchors.len(), 1);
    assert!(brain.zoomed);
    assert!(brain.whole_screen);
    assert!(brain.show_coarse_grid);
    assert_eq!(brain.controlled_tab_id, Some(73));

    let expected_view = window_view(brain.target.as_deref(), false);
    brain.begin_job(12, None, false);
    assert_eq!(brain.current_turn_id, Some(12));
    assert!(brain.trail.is_empty());
    assert!(brain.recent_actions.is_empty());
    assert!(brain.advice_latches.is_empty());
    assert!(brain.prev_state_sig.is_none());
    assert_eq!(brain.wait_accum, 0.0);
    assert!(brain.last_click.is_none());
    assert!(brain.click_before.is_none());
    assert!(brain.active_action.is_none());
    assert!(brain.controlled_tab_id.is_none());
    assert!(brain.controlled_document_id.is_none());
    assert!(brain.anchors.is_empty());
    assert_eq!(brain.next_anchor_id, 42);
    assert!(!brain.zoomed);
    assert!(!brain.whole_screen);
    assert!(!brain.show_coarse_grid);
    assert!(same_view(brain.view, expected_view));
}

#[test]
fn new_turn_binds_browser_tools_to_the_source_frame_tab() {
    let mut brain = Brain::new(None);
    let source = FrameSource {
        frame_id: 81,
        surface: super::super::controller::world::SurfaceIdentity::Browser {
            tab_id: 37,
            document_id: "document-81".into(),
            window: browser_window(),
        },
    };

    brain.begin_job(44, Some(source.clone()), false);
    assert_eq!(brain.source_frame, Some(source));
    assert_eq!(brain.controlled_tab_id, Some(37));
    assert_eq!(brain.controlled_document_id.as_deref(), Some("document-81"));

    let native_source = FrameSource::native(82, (5, 6, 7));
    brain.begin_job(44, Some(native_source.clone()), false);
    assert_eq!(brain.source_frame, Some(native_source));
    assert_eq!(brain.controlled_tab_id, Some(37));
    assert_eq!(brain.controlled_document_id.as_deref(), Some("document-81"));
}
