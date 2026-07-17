use super::*;

fn typed(error: anyhow::Error) -> Value {
    pointer_error_response(error)
}

#[test]
fn cancellation_before_any_dispatch_is_a_proven_noop() {
    let cancel = AtomicBool::new(true);
    let mut events = Vec::new();
    let result = run_click_with(
        10.0,
        20.0,
        false,
        &cancel,
        |command, mode| {
            events.push((command, mode));
            Ok(())
        },
        || Ok(()),
    );

    let value = typed(result.unwrap_err());
    assert!(events.is_empty(), "no input may be dispatched: {events:?}");
    assert_eq!(value["cancelled"], true);
    assert_eq!(value["effect_may_have_occurred"], false);
    assert_eq!(value["release_attempted"], false);
}

#[test]
fn cancellation_at_press_edge_reports_the_dispatched_hover() {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let result = run_click_with(
        10.0,
        20.0,
        false,
        &cancel,
        |command, mode| {
            events.push((command, mode));
            Ok(())
        },
        || {
            cancel.store(true, Ordering::SeqCst);
            Ok(())
        },
    );

    let value = typed(result.unwrap_err());
    assert_eq!(events.len(), 1, "only the hover move: {events:?}");
    assert_eq!(value["cancelled"], true);
    assert_eq!(value["effect_may_have_occurred"], true);
    assert_eq!(value["release_attempted"], false);
}

#[test]
fn drag_cancellation_before_any_dispatch_is_a_proven_noop() {
    let cancel = AtomicBool::new(true);
    let mut events = Vec::new();
    let result = run_drag_with(
        (3.0, 4.0),
        (30.0, 40.0),
        &cancel,
        |command, mode| {
            events.push((command, mode));
            Ok(())
        },
        || Ok(()),
        |_| false,
    );

    let value = typed(result.unwrap_err());
    assert!(events.is_empty(), "no input may be dispatched: {events:?}");
    assert_eq!(value["cancelled"], true);
    assert_eq!(value["effect_may_have_occurred"], false);
}

#[test]
fn click_cancellation_after_press_always_uses_cleanup_release() {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let result = run_click_with(
        10.0,
        20.0,
        false,
        &cancel,
        |command, mode| {
            events.push((command, mode));
            if matches!(command, PointerCommand::Press { .. }) {
                cancel.store(true, Ordering::SeqCst);
            }
            Ok(())
        },
        || Ok(()),
    );

    let value = typed(result.unwrap_err());
    assert!(matches!(
        events.last(),
        Some((PointerCommand::Release { .. }, DispatchMode::Cleanup))
    ));
    assert_eq!(value["cancelled"], true);
    assert_eq!(value["effect_may_have_occurred"], true);
    assert_eq!(value["release_succeeded"], true);
}

#[test]
fn failed_press_reply_still_gets_a_cleanup_release() {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let result = run_click_with(
        4.0,
        5.0,
        false,
        &cancel,
        |command, mode| {
            events.push((command, mode));
            if matches!(command, PointerCommand::Press { .. }) {
                anyhow::bail!("reply lost after dispatch");
            }
            Ok(())
        },
        || Ok(()),
    );

    let value = typed(result.unwrap_err());
    assert!(matches!(
        events.last(),
        Some((PointerCommand::Release { .. }, DispatchMode::Cleanup))
    ));
    assert_eq!(value["cancelled"], false);
    assert_eq!(value["effect_may_have_occurred"], true);
    assert_eq!(value["release_succeeded"], true);
}

#[test]
fn drag_move_failure_releases_at_last_confirmed_point() {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let mut held_moves = 0;
    let result = run_drag_with(
        (0.0, 0.0),
        (28.0, 28.0),
        &cancel,
        |command, mode| {
            events.push((command, mode));
            if matches!(command, PointerCommand::Move { held: true, .. }) {
                held_moves += 1;
                if held_moves == 2 {
                    anyhow::bail!("transport failed");
                }
            }
            Ok(())
        },
        || Ok(()),
        |_| false,
    );

    let value = typed(result.unwrap_err());
    assert_eq!(value["effect_may_have_occurred"], true);
    assert_eq!(value["release_succeeded"], true);
    assert!(matches!(
        events.last(),
        Some((PointerCommand::Release { x, y, .. }, DispatchMode::Cleanup))
            if (*x - 1.0).abs() < f64::EPSILON && (*y - 1.0).abs() < f64::EPSILON
    ));
}

#[test]
fn drag_cancellation_during_hold_releases_without_gliding() {
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let result = run_drag_with(
        (3.0, 4.0),
        (30.0, 40.0),
        &cancel,
        |command, mode| {
            events.push((command, mode));
            Ok(())
        },
        || Ok(()),
        |_| {
            cancel.store(true, Ordering::SeqCst);
            true
        },
    );

    let value = typed(result.unwrap_err());
    assert_eq!(value["cancelled"], true);
    assert_eq!(value["effect_may_have_occurred"], true);
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[2],
        (PointerCommand::Release { .. }, DispatchMode::Cleanup)
    ));
}

#[test]
fn failed_cleanup_is_reported_instead_of_claiming_release() {
    let cancel = AtomicBool::new(false);
    let result = run_click_with(
        1.0,
        2.0,
        false,
        &cancel,
        |command, mode| {
            if matches!(command, PointerCommand::Release { .. }) && mode == DispatchMode::Cleanup {
                anyhow::bail!("cleanup unavailable");
            }
            Ok(())
        },
        || Ok(()),
    );

    let value = typed(result.unwrap_err());
    assert_eq!(value["effect_may_have_occurred"], true);
    assert_eq!(value["release_attempted"], true);
    assert_eq!(value["release_succeeded"], false);
}
