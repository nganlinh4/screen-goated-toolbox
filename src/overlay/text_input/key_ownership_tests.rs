use super::{KeyOwnership, Modifiers};

#[test]
fn foreground_press_stays_foreground_through_activation_and_repeat() {
    let mut keys = KeyOwnership::default();
    assert!(!keys.event(65, 30, 0, true, None).consume);
    let repeat = keys.event(65, 30, 0, true, Some(1));
    assert!(!repeat.consume && !repeat.deliver);
    assert!(!keys.event(65, 30, 0, false, Some(1)).consume);
    let next = keys.event(65, 30, 0, true, Some(1));
    assert!(next.consume && next.deliver);
}

#[test]
fn captured_press_and_repeats_remain_captured_through_hide_or_replacement() {
    let mut keys = KeyOwnership::default();
    assert!(keys.event(65, 30, 0, true, Some(1)).deliver);
    assert!(keys.event(65, 30, 0, true, Some(1)).deliver);
    let replacement = keys.event(65, 30, 0, true, Some(2));
    assert!(replacement.consume && !replacement.deliver);
    let hidden = keys.event(65, 30, 0, false, None);
    assert!(hidden.consume && !hidden.deliver);
    assert!(!keys.event(65, 30, 0, true, None).consume);
}

#[test]
fn preexisting_keys_and_independent_modifier_sides_are_tracked() {
    let mut keys = KeyOwnership::default();
    keys.seed(|vk| vk == 0xa2);
    let right = keys.event(0x11, 29, 1, true, Some(1));
    assert!(right.consume && right.modifiers.ctrl);
    let left_up = keys.event(0x11, 29, 0, false, Some(1));
    assert!(!left_up.consume && left_up.modifiers.ctrl);
    assert!(!keys.event(0x11, 29, 1, false, Some(1)).modifiers.ctrl);
    keys.event(0x10, 42, 0, true, Some(1));
    keys.event(0x10, 54, 0, true, Some(1));
    assert!(keys.event(0x10, 42, 0, false, Some(1)).modifiers.shift);
    assert!(!keys.event(0x10, 54, 0, false, Some(1)).modifiers.shift);
}

#[test]
fn injected_events_pair_like_other_events_and_teardown_resets_tracking() {
    let mut keys = KeyOwnership::default();
    assert!(keys.event(65, 30, 0x10, true, Some(1)).consume);
    assert!(keys.event(65, 30, 0x12, false, None).consume);
    assert!(keys.event(66, 48, 0, true, Some(1)).consume);
    keys = KeyOwnership::default();
    keys.seed(|vk| vk == 66);
    assert!(!keys.event(66, 48, 0, false, Some(2)).consume);
    assert!(!keys.event(300, 0, 0, true, Some(2)).consume);
}

#[test]
fn deferred_key_keeps_the_modifiers_seen_by_the_hook() {
    for bits in 0..16 {
        let modifiers = Modifiers::from_bits(bits);
        assert_eq!(modifiers.to_bits(), bits);
        assert_eq!(Modifiers::from_bits(modifiers.to_bits()), modifiers);
    }
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    assert_eq!(Modifiers::from_bits(shift.to_bits()), shift);
}
