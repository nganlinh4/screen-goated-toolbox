use super::*;

#[cfg(debug_assertions)]
#[test]
fn latest_diagnostic_request_fences_slower_earlier_request() {
    let request = Mutex::new(DiagnosticAdmission::default());
    let first = reserve_replay_request(&request, 0, || false).unwrap();
    let second = reserve_replay_request(&request, first, || false).unwrap();
    let current = request.lock().unwrap();
    assert!(!current.accepts_replay(first));
    assert!(current.accepts_replay(second));
}

#[cfg(debug_assertions)]
#[test]
fn selector_claim_fences_replay_even_if_busy_was_checked_earlier() {
    let mut admission = DiagnosticAdmission::default();
    let old = admission.reserve_replay().unwrap();
    let selector = admission.begin_selector().unwrap();
    assert!(!admission.accepts_replay(old));
    assert!(admission.reserve_replay().is_err());
    assert!(admission.finish_selector(selector));
    assert!(!admission.accepts_replay(old));
    let after_cancel = admission.reserve_replay().unwrap();
    assert!(admission.accepts_replay(after_cancel));
}

#[cfg(debug_assertions)]
#[test]
fn selector_claim_and_cancel_fence_a_previously_observed_idle_request() {
    let admission = Mutex::new(DiagnosticAdmission::default());
    let observed = admission.lock().unwrap().generation;
    let selector = admission.lock().unwrap().begin_selector().unwrap();
    assert!(reserve_replay_request(&admission, observed, || true).is_err());
    assert!(admission.lock().unwrap().finish_selector(selector));
    // Even though the selector was canceled and the app is idle again, an
    // old lab-queue busy check must not reserve work after it.
    assert!(reserve_replay_request(&admission, observed, || false).is_err());
    let current = admission.lock().unwrap().generation;
    assert!(reserve_replay_request(&admission, current, || false).is_ok());
}

#[cfg(debug_assertions)]
#[test]
fn stop_during_selection_retires_both_replay_and_selector() {
    let mut admission = DiagnosticAdmission::default();
    let old = admission.reserve_replay().unwrap();
    let selector = admission.begin_selector().unwrap();
    admission.invalidate();
    assert!(!admission.accepts_replay(old));
    assert!(admission.reserve_replay().is_err());
    assert!(!admission.finish_selector(selector));
    let newer = admission.reserve_replay().unwrap();
    assert!(admission.accepts_replay(newer));
}

#[cfg(debug_assertions)]
#[test]
fn stop_before_ocr_finishes_invalidates_old_but_not_later_request() {
    let mut admission = DiagnosticAdmission::default();
    let old = admission.reserve_replay().unwrap();
    assert!(super::super::admit_stop(&mut admission, &None, None));
    let newer = admission.reserve_replay().unwrap();
    assert!(!admission.accepts_replay(old));
    assert!(admission.accepts_replay(newer));
}

#[cfg(debug_assertions)]
#[test]
fn stale_session_stop_does_not_retire_newer_replay_request() {
    let old = super::super::tests::test_session();
    let newer = super::super::tests::test_session();
    let slot = Some(newer);
    let mut admission = DiagnosticAdmission::default();
    let request = admission.reserve_replay().unwrap();
    assert!(!super::super::admit_stop(&mut admission, &slot, Some(&old)));
    assert!(admission.accepts_replay(request));
    assert!(super::super::admit_stop(&mut admission, &slot, None));
    assert!(!admission.accepts_replay(request));
}

#[cfg(debug_assertions)]
#[test]
fn stale_lab_commands_are_rejected_after_newer_selector_or_user_toggle() {
    let mut admission = DiagnosticAdmission::default();
    let observed = admission.generation;
    assert!(super::super::lab_request_current(
        &admission, observed, false
    ));
    let selector = admission.begin_selector().unwrap();
    assert!(!super::super::lab_request_current(
        &admission, observed, false
    ));
    assert!(admission.finish_selector(selector));
    assert!(!super::super::lab_request_current(
        &admission, observed, false
    ));

    let next = admission.generation;
    assert!(super::super::lab_request_current(&admission, next, false));
    // A normal interactive toggle is a newer action even without a selector.
    let session = super::super::tests::test_session();
    let slot = Some(Arc::clone(&session));
    assert_eq!(
        super::super::toggle_existing_admitted(&mut admission, &slot, "new", None, false),
        Some(true)
    );
    assert!(!super::super::lab_request_current(&admission, next, false));
    assert_eq!(
        super::super::toggle_existing_admitted(&mut admission, &slot, "old", Some(next), false),
        None
    );
    assert_eq!(session.view.lock().unwrap().hotkey, "new");
    assert!(!super::super::lab_request_current(
        &admission,
        admission.generation,
        true
    ));
}

#[cfg(debug_assertions)]
#[test]
fn replay_rejects_interactive_session_before_window_creation_and_during_teardown() {
    use windows::Win32::Foundation::RECT;
    let rect = RECT {
        left: 0,
        top: 0,
        right: 32,
        bottom: 32,
    };
    let interactive = super::super::new_session(
        rect,
        rect,
        "en".into(),
        String::new(),
        super::super::SessionKind::Interactive,
    );
    let mut slot = Some(Arc::clone(&interactive));
    assert_eq!(interactive.native_window.load(Ordering::Acquire), 0);
    assert!(diagnostic_session(&mut slot, rect).is_err());
    interactive.native_window.store(12, Ordering::Release);
    assert!(diagnostic_session(&mut slot, rect).is_err());
    interactive.native_window.store(0, Ordering::Release);
    assert!(diagnostic_session(&mut slot, rect).is_err());
    assert!(Arc::ptr_eq(slot.as_ref().unwrap(), &interactive));
    assert_eq!(interactive.view.lock().unwrap().epoch, 1);
    let frame = super::super::tests::frame(&[("test", 100)]);
    assert!(replay_test_frame(&interactive, 1, &frame, &frame.image).is_err());
}

#[cfg(debug_assertions)]
#[test]
fn diagnostic_session_is_reused_only_while_owned_and_live() {
    use windows::Win32::Foundation::RECT;
    let rect = RECT {
        left: 0,
        top: 0,
        right: 32,
        bottom: 32,
    };
    let mut slot = None;
    let (first, created) = diagnostic_session(&mut slot, rect).unwrap();
    assert!(created);
    let (same, created) = diagnostic_session(&mut slot, rect).unwrap();
    assert!(!created);
    assert!(Arc::ptr_eq(&first, &same));
    first.stop.store(true, Ordering::Release);
    assert!(diagnostic_session(&mut slot, rect).is_err());
    assert!(Arc::ptr_eq(slot.as_ref().unwrap(), &first));
}

#[test]
fn older_failure_cannot_retry_a_newer_capture_request() {
    let newer = CapturedFrame {
        id: 2,
        image: Arc::new(image::RgbaImage::new(8, 8)),
    };
    let requested = Some(newer.clone());
    assert!(!is_current_request(requested.as_ref(), 1));
    assert!(!should_request_ocr(
        requested.as_ref(),
        &newer,
        None,
        Duration::from_millis(180),
        Instant::now(),
        true,
    ));
    assert!(is_current_request(requested.as_ref(), 2));
    let retry = CapturedFrame {
        id: 3,
        image: Arc::clone(&newer.image),
    };
    assert!(should_request_ocr(
        requested.as_ref(),
        &retry,
        None,
        Duration::from_millis(180),
        Instant::now(),
        true,
    ));
}

#[test]
fn initial_empty_observation_gets_one_fresh_retry_and_resets_on_change() {
    let mut retry = EmptyConfirmation::default();
    let empty = super::super::tests::frame(&[]);
    retry.observe(&empty);
    assert!(retry.pending());
    let interval = Duration::from_millis(180);
    let first_capture = CapturedFrame {
        id: empty.capture_id,
        image: Arc::clone(&empty.image),
    };
    assert!(!should_request_ocr(
        Some(&first_capture),
        &first_capture,
        None,
        interval,
        Instant::now(),
        retry.pending()
    ));
    let fresh_empty = super::super::tests::frame(&[]);
    let next_capture = CapturedFrame {
        id: fresh_empty.capture_id,
        image: Arc::clone(&fresh_empty.image),
    };
    assert!(should_request_ocr(
        Some(&first_capture),
        &next_capture,
        None,
        interval,
        Instant::now(),
        retry.pending()
    ));
    retry.retry_requested = true;
    assert!(!retry.pending());
    retry.observe(&fresh_empty);
    assert!(!retry.pending());
    let detected = super::super::tests::frame(&[("caption", 100)]);
    retry.observe(&detected);
    assert!(!retry.pending());
    let mut scene = super::super::tracks::Scene::default();
    scene.observe(1, Arc::clone(&detected), 200, &detected.image);
    assert!(!scene.tracks[0].dispatchable());
    let confirmed = super::super::tests::frame(&[("caption", 100)]);
    scene.observe(1, Arc::clone(&confirmed), 400, &confirmed.image);
    assert!(scene.tracks[0].dispatchable());
    let mut changed = super::super::tests::frame(&[]);
    Arc::make_mut(&mut Arc::get_mut(&mut changed).unwrap().image).put_pixel(
        0,
        0,
        image::Rgba([1, 2, 3, 255]),
    );
    retry.observe(&changed);
    assert!(retry.pending());
}

#[test]
fn confirmation_requires_a_new_capture_even_when_pixels_match() {
    let first = CapturedFrame {
        id: 1,
        image: Arc::new(image::RgbaImage::new(8, 8)),
    };
    let next = CapturedFrame {
        id: 2,
        image: Arc::new(first.image.as_ref().clone()),
    };
    let now = Instant::now();
    let watch = TextWatch::new(&first.image, &[], now);
    let interval = Duration::from_millis(180);
    assert!(!should_request_ocr(
        Some(&first),
        &first,
        Some(&watch),
        interval,
        now,
        true
    ));
    assert!(should_request_ocr(
        Some(&first),
        &next,
        Some(&watch),
        interval,
        now,
        true
    ));
    assert!(!should_request_ocr(
        Some(&first),
        &next,
        Some(&watch),
        interval,
        now,
        false
    ));
    assert!(!should_request_ocr(
        Some(&first),
        &next,
        Some(&watch),
        interval - Duration::from_millis(1),
        now,
        true
    ));
}

#[cfg(debug_assertions)]
#[test]
fn deferred_lab_effect_cannot_follow_newer_selector() {
    let gate = Mutex::new(DiagnosticAdmission::default());
    let observed = gate.lock().unwrap().generation;
    {
        let mut admission = gate.lock().unwrap();
        let selector = admission.begin_selector().unwrap();
        assert!(admission.finish_selector(selector));
    }
    let mut dispatched = false;
    assert!(
        admit_lab_effect_on(&gate, observed, || {
            dispatched = true;
            Some(())
        })
        .is_none()
    );
    assert!(!dispatched);
}

#[cfg(debug_assertions)]
#[test]
fn newer_lab_submission_wins_in_either_completion_order() {
    for newer_finishes_first in [false, true] {
        let gate = Mutex::new(DiagnosticAdmission::default());
        let first = reserve_lab_request_on(&gate, 0, || Some(())).unwrap();
        let second = reserve_lab_request_on(&gate, first, || Some(())).unwrap();
        assert!(second > first);
        let mut published = Vec::new();
        let order = if newer_finishes_first {
            [second, first]
        } else {
            [first, second]
        };
        for ticket in order {
            let _ = admit_lab_effect_on(&gate, ticket, || {
                published.push(ticket);
                Some(())
            });
        }
        assert_eq!(published, vec![second]);
    }
}

#[cfg(debug_assertions)]
#[test]
fn new_reservation_rejects_publication_after_staging() {
    let gate = Mutex::new(DiagnosticAdmission::default());
    let first = reserve_lab_request_on(&gate, 0, || Some(())).unwrap();
    let (_, generation) = admit_lab_effect_on(&gate, first, || Some(())).unwrap();
    let generation = generation.unwrap();
    // The older replay has started and staged output when a newer request arrives.
    let newer = reserve_lab_request_on(&gate, generation, || Some(())).unwrap();
    let mut published = false;
    assert!(with_lab_generation_on(&gate, generation, || published = true).is_none());
    assert!(!published);
    assert!(with_lab_generation_on(&gate, newer, || ()).is_some());
}

#[cfg(debug_assertions)]
#[test]
fn slow_validation_fences_older_publication_without_discarding_it_on_malformed_input() {
    for valid_newer in [false, true] {
        let gate = std::sync::Arc::new(Mutex::new(DiagnosticAdmission::default()));
        let first = reserve_lab_request_on(&gate, 0, || Some(())).unwrap();
        let (_, generation) = admit_lab_effect_on(&gate, first, || Some(())).unwrap();
        let generation = generation.unwrap();
        let validation = super::super::super::lab_queue::begin_validation();
        let worker_gate = std::sync::Arc::clone(&gate);
        let (entered, waiting) = mpsc::channel();
        let (sent, received) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            let _permit = super::super::super::lab_queue::validation_permit();
            sent.send(with_lab_generation_on(&worker_gate, generation, || ()).is_some())
                .unwrap();
        });
        waiting.recv().unwrap();
        assert!(received.recv_timeout(Duration::from_millis(30)).is_err());
        if valid_newer {
            reserve_lab_request_on(&gate, generation, || Some(())).unwrap();
        }
        drop(validation);
        assert_eq!(received.recv().unwrap(), !valid_newer);
        worker.join().unwrap();
    }
}

#[cfg(debug_assertions)]
#[test]
fn lab_effect_holds_capture_claim_between_validation_and_mutation() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let gate = Mutex::new(DiagnosticAdmission::default());
    let observed = gate.lock().unwrap().generation;
    let claimed = Arc::new(AtomicBool::new(false));
    let selection = Arc::clone(&claimed);
    let (attempt, wait) = mpsc::channel();
    let (result, finished) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        wait.recv().unwrap();
        result
            .send(
                selection
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok(),
            )
            .unwrap();
    });
    assert!(
        admit_lab_effect_on(&gate, observed, || {
            assert!(
                claimed
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            );
            attempt.send(()).unwrap();
            assert!(!finished.recv().unwrap());
            claimed.store(false, Ordering::SeqCst);
            Some(())
        })
        .is_some()
    );
    worker.join().unwrap();
}
