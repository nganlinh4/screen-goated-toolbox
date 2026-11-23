use super::*;

fn admit(state: &mut CircuitState, key: &str, kind: Kind, now: Instant) -> Ticket {
    state.admit(key, kind, now).unwrap()
}

fn is_blocked(result: Result<Ticket, AdmissionError>) -> bool {
    matches!(result, Err(AdmissionError::CoolingDown(_)))
}

#[test]
fn unrelated_surfaces_cannot_clear_each_others_cooldowns() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let one = admit(&mut state, "one", Kind::Authoritative, now);
    state.finish(one, Outcome::Timeout, now);
    let two = admit(&mut state, "two", Kind::Authoritative, now);
    state.finish(two, Outcome::Timeout, now);
    assert!(is_blocked(state.admit("one", Kind::Authoritative, now)));
    assert!(is_blocked(state.admit("two", Kind::Optional, now)));
    assert!(
        state
            .admit("one", Kind::Authoritative, now + COOLDOWN)
            .is_ok()
    );
}

#[test]
fn optional_timeout_does_not_block_authoritative_but_authoritative_blocks_optional() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let optional = admit(&mut state, "one", Kind::Optional, now);
    state.finish(optional, Outcome::Timeout, now);
    assert!(is_blocked(state.admit("one", Kind::Optional, now)));
    let authoritative = admit(&mut state, "one", Kind::Authoritative, now);
    state.finish(authoritative, Outcome::Success, now);
    let optional = admit(&mut state, "one", Kind::Optional, now);
    state.release(optional);
    let authoritative = admit(&mut state, "one", Kind::Authoritative, now);
    state.finish(authoritative, Outcome::Timeout, now);
    assert!(is_blocked(state.admit("one", Kind::Optional, now)));
    assert!(is_blocked(state.admit("one", Kind::Authoritative, now)));
}

#[test]
fn old_success_does_not_clear_a_new_concurrent_timeout() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let old = admit(&mut state, "one", Kind::Optional, now);
    let newer = admit(&mut state, "one", Kind::Authoritative, now);
    state.finish(newer, Outcome::Timeout, now);
    state.finish(old, Outcome::Success, now);
    assert!(is_blocked(state.admit("one", Kind::Optional, now)));
}

#[test]
fn later_timeout_survives_a_success_completed_first() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let old = admit(&mut state, "one", Kind::Optional, now);
    let newer = admit(&mut state, "one", Kind::Authoritative, now);
    state.finish(old, Outcome::Success, now);
    state.finish(newer, Outcome::Timeout, now);
    assert!(is_blocked(state.admit("one", Kind::Authoritative, now)));
}

#[test]
fn success_cannot_clear_a_replaced_optional_timeout() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let optional_one = admit(&mut state, "one", Kind::Optional, now);
    state.finish(optional_one, Outcome::Timeout, now);
    let authoritative = admit(&mut state, "one", Kind::Authoritative, now);
    let later = now + COOLDOWN;
    let optional_two = admit(&mut state, "one", Kind::Optional, later);
    state.finish(optional_two, Outcome::Timeout, later);
    state.finish(authoritative, Outcome::Success, later);
    assert!(is_blocked(state.admit("one", Kind::Optional, later)));
}

#[test]
fn each_surface_has_separate_bounded_authoritative_and_optional_slots() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let optional = admit(&mut state, "one", Kind::Optional, now);
    assert!(matches!(
        state.admit("one", Kind::Optional, now),
        Err(AdmissionError::InFlight)
    ));
    let authoritative = admit(&mut state, "one", Kind::Authoritative, now);
    assert!(matches!(
        state.admit("one", Kind::Authoritative, now),
        Err(AdmissionError::InFlight)
    ));
    state.release(optional);
    assert!(state.admit("one", Kind::Optional, now).is_ok());
    state.release(authoritative);
}

#[test]
fn optional_surfaces_cannot_exhaust_authoritative_capacity() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    for index in 0..OPTIONAL_SURFACE_LIMIT {
        let _ = admit(&mut state, &index.to_string(), Kind::Optional, now);
    }
    assert!(matches!(
        state.admit("another-optional", Kind::Optional, now),
        Err(AdmissionError::Capacity)
    ));
    assert!(
        state
            .admit("authoritative", Kind::Authoritative, now)
            .is_ok()
    );
}

#[test]
fn capacity_reserves_in_flight_surfaces_and_recovers_after_release() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let tickets: Vec<_> = (0..MAX_SURFACES)
        .map(|index| admit(&mut state, &index.to_string(), Kind::Authoritative, now))
        .collect();
    assert!(matches!(
        state.admit("extra", Kind::Authoritative, now),
        Err(AdmissionError::Capacity)
    ));
    let mut tickets = tickets.into_iter();
    state.release(tickets.next().unwrap());
    assert!(state.admit("extra", Kind::Authoritative, now).is_ok());
    assert!(matches!(
        state.admit(&"x".repeat(MAX_KEY_BYTES + 1), Kind::Optional, now),
        Err(AdmissionError::Capacity)
    ));
}

#[test]
fn live_cooldowns_are_not_evicted_at_capacity() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    for index in 0..MAX_SURFACES {
        let ticket = admit(&mut state, &index.to_string(), Kind::Authoritative, now);
        state.finish(ticket, Outcome::Timeout, now);
    }
    assert!(matches!(
        state.admit("extra", Kind::Optional, now),
        Err(AdmissionError::Capacity)
    ));
    assert!(is_blocked(state.admit("0", Kind::Authoritative, now)));
    assert!(state.admit("extra", Kind::Optional, now + COOLDOWN).is_ok());
}

#[test]
fn a_worker_still_running_after_its_timeout_keeps_the_surface_slot() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let ticket = admit(&mut state, "one", Kind::Authoritative, now);
    state.record(&ticket, Outcome::Timeout, now);
    let after_cooldown = now + COOLDOWN;
    assert!(matches!(
        state.admit("one", Kind::Authoritative, after_cooldown),
        Err(AdmissionError::InFlight)
    ));
    state.release(ticket);
    assert!(
        state
            .admit("one", Kind::Authoritative, after_cooldown)
            .is_ok()
    );
}

#[test]
fn a_timeout_reported_after_its_worker_exited_still_cools_down() {
    let now = Instant::now();
    let mut state = CircuitState::default();
    let ticket = admit(&mut state, "one", Kind::Optional, now);
    state.release(ticket.clone());
    assert!(state.surfaces.is_empty());
    state.record(&ticket, Outcome::Timeout, now);
    assert!(is_blocked(state.admit("one", Kind::Optional, now)));
}

#[test]
fn worker_lease_releases_the_global_slot_only_when_dropped() {
    let surface = ResolvedSurface {
        hwnd: usize::MAX - 7,
        pid: u32::MAX - 7,
    };
    let (permit, lease) = super::admit(&surface, Kind::Optional).unwrap();
    permit.timeout();
    assert!(matches!(
        super::admit(&surface, Kind::Optional),
        Err(AdmissionError::CoolingDown(_))
    ));
    assert!(
        state()
            .surfaces
            .get(&surface.key())
            .is_some_and(|entry| entry.optional_in_flight)
    );
    drop(lease);
    assert!(
        state()
            .surfaces
            .get(&surface.key())
            .is_some_and(|entry| !entry.optional_in_flight && entry.in_flight == 0)
    );
}

fn find_editor(selector: &str) -> anyhow::Result<usize> {
    match selector {
        "Editor" | "editor.exe" | "@hwnd:5:7" => Ok(5),
        _ => Err(anyhow::anyhow!("no window")),
    }
}

#[test]
fn selector_aliases_and_the_foreground_resolve_to_one_surface() {
    let active = |target, foreground| {
        super::super::active_window_with(target, find_editor, foreground, |candidate, root| {
            candidate == 11 && root == 5
        })
        .unwrap()
    };
    for selector in ["Editor", "editor.exe", "@hwnd:5:7"] {
        assert_eq!(active(Some(selector), 9), 5);
        assert_eq!(active(Some(selector), 5), 5);
        // An owned foreground dialog is the tree actually enumerated.
        assert_eq!(active(Some(selector), 11), 11);
    }
    assert_eq!(active(None, 5), 5);
    assert_eq!(active(None, 9), 9);
    assert!(
        super::super::active_window_with(Some("missing"), find_editor, 5, |_, _| false).is_err()
    );

    let mut state = CircuitState::default();
    let now = Instant::now();
    let by_title = ResolvedSurface { hwnd: 5, pid: 7 };
    let ticket = admit(&mut state, &by_title.key(), Kind::Authoritative, now);
    state.finish(ticket, Outcome::Timeout, now);
    let from_foreground = ResolvedSurface {
        hwnd: active(None, 5),
        pid: 7,
    };
    assert!(is_blocked(state.admit(
        &from_foreground.key(),
        Kind::Optional,
        now
    )));
}
