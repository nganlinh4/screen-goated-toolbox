use super::*;
use windows::Win32::Foundation::HWND;

#[test]
fn closed_group_is_removed_and_cancelled_even_before_any_reveal() {
    use std::cell::RefCell;
    let events = RefCell::new(Vec::new());
    let closed = retire_closed_group(
        false,
        || events.borrow_mut().push("remove"),
        || events.borrow_mut().push("cancel"),
    );
    assert!(closed);
    assert_eq!(*events.borrow(), ["remove", "cancel"]);
    assert!(!retire_closed_group(
        true,
        || panic!("live group was removed"),
        || panic!("live group was cancelled"),
    ));
}

#[test]
fn late_renderer_failure_reports_its_cause_before_cancelling_inference() {
    use std::cell::RefCell;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let (_ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    let (command_sender, _commands) = render_mailbox::channel().unwrap();
    let mut overlay = TranslationOverlay {
        sender: command_sender,
        completion: receiver,
        ready,
        is_ready: true,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    };
    let events = RefCell::new(Vec::new());
    let cancel = std::sync::atomic::AtomicBool::new(false);
    fail_after_setup(
        "mailbox wait failed",
        |error| {
            events.borrow_mut().push("report");
            sender.send(Err(error.to_string())).unwrap();
        },
        || events.borrow_mut().push("close"),
        |cause| {
            assert_eq!(cause, "mailbox wait failed");
            events.borrow_mut().push("end");
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        },
    );
    assert_eq!(*events.borrow(), ["report", "close", "end"]);
    // The job token is the one legacy and incremental inference poll.
    assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));
    let error = overlay.check_ready().unwrap_err();
    assert_eq!(error.to_string(), "mailbox wait failed");
}

#[test]
fn missing_controller_is_a_renderer_error() {
    assert!(checked_controller(HWND::default()).is_err());
}

#[test]
fn renderer_startup_error_survives_closed_command_channel() {
    let (sender, receiver) = render_mailbox::channel().unwrap();
    drop(receiver);
    let (completion_sender, completion) = std::sync::mpsc::sync_channel(1);
    completion_sender
        .send(Err("controller failed".into()))
        .unwrap();
    let (_ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    let overlay = TranslationOverlay {
        sender,
        completion,
        ready,
        is_ready: false,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    };
    let error = overlay
        .complete(TranslationDocument {
            regions: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "controller failed");
}

#[test]
fn renderer_setup_error_is_reported_before_translation_dispatch() {
    let (sender, _commands) = render_mailbox::channel().unwrap();
    let (_completion_sender, completion) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    ready_sender.send(Err("scene failed".into())).unwrap();
    let mut overlay = TranslationOverlay {
        sender,
        completion,
        ready,
        is_ready: false,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    };
    let error = overlay.wait_ready(&AtomicBool::new(false)).unwrap_err();
    assert_eq!(error.to_string(), "scene failed");
}

#[test]
fn pending_setup_gates_dispatch_until_success_or_failure() {
    for setup in [Ok(()), Err("delayed failure".to_string())] {
        let (sender, _commands) = render_mailbox::channel().unwrap();
        let (_completion_sender, completion) = std::sync::mpsc::sync_channel(1);
        let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
        let (entered_sender, entered) = std::sync::mpsc::sync_channel(1);
        let (dispatch_sender, dispatch) = std::sync::mpsc::sync_channel(1);
        let mut overlay = TranslationOverlay {
            sender,
            completion,
            ready,
            is_ready: false,
            setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
        };
        let succeeds = setup.is_ok();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                entered_sender.send(()).unwrap();
                let result = overlay.wait_ready(&AtomicBool::new(false));
                dispatch_sender
                    .send(result.map_err(|error| error.to_string()))
                    .unwrap();
            });
            entered.recv().unwrap();
            assert!(matches!(dispatch.try_recv(), Err(TryRecvError::Empty)));
            ready_sender.send(setup).unwrap();
            let result = dispatch.recv().unwrap();
            assert_eq!(result.is_ok(), succeeds);
            if let Err(error) = result {
                assert_eq!(error, "delayed failure");
            }
        });
    }
}

#[test]
fn renderer_readiness_reports_disconnect_and_respects_cancellation() {
    let (sender, _commands) = render_mailbox::channel().unwrap();
    let (_completion_sender, completion) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    let mut overlay = TranslationOverlay {
        sender,
        completion,
        ready,
        is_ready: false,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    };
    assert!(
        overlay
            .wait_ready(&AtomicBool::new(true))
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    drop(ready_sender);
    assert!(
        overlay
            .check_ready()
            .unwrap_err()
            .to_string()
            .contains("before setup")
    );
}

#[test]
fn renderer_success_keeps_completion_for_complete_and_detects_late_failure() {
    let (sender, _commands) = render_mailbox::channel().unwrap();
    let (completion_sender, completion) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    ready_sender.send(Ok(())).unwrap();
    let mut overlay = TranslationOverlay {
        sender,
        completion,
        ready,
        is_ready: false,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    };
    overlay.wait_ready(&AtomicBool::new(false)).unwrap();
    completion_sender
        .send(Err("renderer failed".into()))
        .unwrap();
    assert_eq!(
        overlay.check_ready().unwrap_err().to_string(),
        "renderer failed"
    );
}

#[test]
fn renderer_ready_then_complete_returns_the_rendered_count() {
    let (sender, _commands) = render_mailbox::channel().unwrap();
    let (completion_sender, completion) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    ready_sender.send(Ok(())).unwrap();
    let mut overlay = TranslationOverlay {
        sender,
        completion,
        ready,
        is_ready: false,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    };
    overlay.wait_ready(&AtomicBool::new(false)).unwrap();
    completion_sender.send(Ok(7)).unwrap();
    assert_eq!(
        overlay
            .complete(TranslationDocument {
                regions: Vec::new()
            })
            .unwrap(),
        7
    );
}

#[test]
fn unresolved_geometry_cannot_be_covered_by_a_partial_block() {
    let block = PreparedBlock {
        member_ids: vec![1, 2],
        layout: super::super::geometry::PixelRegion {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
        backdrop: String::new(),
        foreground: String::new(),
        preferred_font_size: 12.0,
        vertical_text: false,
        source_regions: Vec::new(),
        source_lane_member_ids: vec![vec![1, 2]],
    };
    let scene = PreparedScene {
        sources: HashMap::new(),
        blocks: Vec::new(),
    };
    let mut translations = HashMap::from([(
        1,
        SegmentTranslation {
            source_text: "source".into(),
            translated_text: "changed".into(),
        },
    )]);
    assert!(component_translation(&block, &scene, &translations, false).is_none());
    translations.insert(
        2,
        SegmentTranslation {
            source_text: "second".into(),
            translated_text: "finished".into(),
        },
    );
    assert_eq!(
        component_translation(&block, &scene, &translations, false),
        Some(vec!["changed finished".into()])
    );
}

#[test]
fn control_anchor_preserves_absolute_screen_coordinates() {
    assert_eq!(
        selection_anchor((-1200, 300), (640, 480)),
        [-1200, 300, 640, 480]
    );
}

#[test]
fn source_equivalent_regions_do_not_require_visual_replacement() {
    let region = TranslationRegion {
        id: 1,
        member_ids: vec![1],
        member_joins: Vec::new(),
        selections: Vec::new(),
        semantic_role: super::super::contract::SemanticRole::Value,
        source_text: "example.com/path".to_string(),
        translated_segments: vec!["example.com/path".to_string()],
        bounds: [0, 0, 10, 10].into(),
        background_color: None,
        text_color: None,
    };
    assert!(!should_render_segment(
        &region.source_text,
        &region.translated_segments[0]
    ));
}

#[test]
fn model_cell_is_recorded_as_individual_detector_segments() {
    let region = TranslationRegion {
        id: 4,
        member_ids: vec![4, 6, 7, 9],
        member_joins: vec![
            super::super::contract::MemberJoin::SameColumn,
            super::super::contract::MemberJoin::SameColumn,
            super::super::contract::MemberJoin::SameColumn,
        ],
        selections: vec![
            super::super::contract::TranslationSelection {
                region_id: 4,
                candidate_id: "4:0".to_string(),
                source_text: "one".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
            super::super::contract::TranslationSelection {
                region_id: 6,
                candidate_id: "6:0".to_string(),
                source_text: "two".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
            super::super::contract::TranslationSelection {
                region_id: 7,
                candidate_id: "7:0".to_string(),
                source_text: "three".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
            super::super::contract::TranslationSelection {
                region_id: 9,
                candidate_id: "9:0".to_string(),
                source_text: "four".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
        ],
        semantic_role: super::super::contract::SemanticRole::Dialogue,
        source_text: "source cell".to_string(),
        translated_segments: vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
            "fourth".to_string(),
        ],
        bounds: [0, 0, 10, 10].into(),
        background_color: None,
        text_color: None,
    };
    let mut translations = HashMap::new();
    record_translations(region, &mut translations);
    assert_eq!(translations.get(&4).unwrap().translated_text, "first");
    assert_eq!(translations.get(&9).unwrap().translated_text, "fourth");
}

#[test]
fn completed_units_resolve_before_the_document_while_unfinished_siblings_wait() {
    let block = PreparedBlock {
        member_ids: vec![1, 2],
        layout: super::super::geometry::PixelRegion {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
        backdrop: String::new(),
        foreground: String::new(),
        preferred_font_size: 12.0,
        vertical_text: false,
        source_regions: vec![],
        source_lane_member_ids: vec![vec![1, 2]],
    };
    let mut sibling = block.clone();
    sibling.member_ids = vec![3];
    sibling.source_lane_member_ids = vec![vec![3]];
    let scene = PreparedScene {
        sources: HashMap::new(),
        blocks: vec![],
    };
    let mut translations = HashMap::new();
    let region = |id, translated: &str| TranslationRegion {
        id,
        member_ids: vec![id],
        member_joins: vec![],
        selections: vec![super::super::contract::TranslationSelection {
            region_id: id,
            candidate_id: format!("{id}:0"),
            source_text: format!("source{id}"),
            bounds: [0, 0, 10, 10].into(),
        }],
        semantic_role: super::super::contract::SemanticRole::Standalone,
        source_text: format!("source{id}"),
        translated_segments: vec![translated.into()],
        bounds: [0, 0, 10, 10].into(),
        background_color: None,
        text_color: None,
    };
    assert!(component_translation(&block, &scene, &translations, true).is_none());
    record_translations(region(1, "first"), &mut translations);
    assert!(component_translation(&block, &scene, &translations, true).is_none());
    record_translations(region(2, "second"), &mut translations);
    assert_eq!(
        component_translation(&block, &scene, &translations, true),
        Some(vec!["first second".into()])
    );
    assert!(component_translation(&sibling, &scene, &translations, true).is_none());
    record_translations(region(3, "third"), &mut translations);
    assert_eq!(
        component_translation(&sibling, &scene, &translations, true),
        Some(vec!["third".into()])
    );
}

#[test]
fn stalled_setup_fails_the_job_after_its_bound_and_late_setup_closes_itself() {
    let (sender, _commands) = render_mailbox::channel().unwrap();
    let (_completion_sender, completion) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready) = std::sync::mpsc::sync_channel(1);
    let mut overlay = TranslationOverlay {
        sender,
        completion,
        ready,
        is_ready: false,
        setup_deadline: Instant::now() + Duration::from_millis(50),
    };
    overlay.check_ready().unwrap();
    let started = Instant::now();
    let error = overlay.wait_ready(&AtomicBool::new(false)).unwrap_err();
    assert!(error.to_string().contains("did not finish"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(overlay);
    // The renderer thread's readiness report is how it learns to close.
    assert!(ready_sender.send(Ok(())).is_err());
}
