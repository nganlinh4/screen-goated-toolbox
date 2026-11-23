//! Keyboard action handlers for the Computer Control executor: `type_text`
//! (slow human typing or Unicode input) and
//! `key_combination` (named-key / chord parsing). The raw `SendInput` key builders
//! live in the parent module and are reached via `super::`.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, VIRTUAL_KEY, VK_RETURN};

use super::super::human_input::{self, HumanProfile, Outcome};

pub(super) mod key_tokens;
mod target_lease;
use key_tokens::parse_key_sequence;
use target_lease::{TargetLease, focus_anchor};

pub(super) fn type_text(
    args: &Value,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Result<Value> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(super::aborted());
    }
    let raw = args
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing text"))?;
    // Text is always literal. Submitting is a separate, explicit effect; neither
    // a newline nor a marker embedded in user text is permission to press Enter.
    let text = raw.to_string();
    let enter = press_enter_requested(args);
    let n = text.chars().count();
    let total_units = text.encode_utf16().count();
    let started_units = Cell::new(0usize);
    super::verify_expected_keyboard_target(args)?;
    // Slow, human-paced per-key typing ONLY when explicitly asked for (a rare field
    // that demands paced keystrokes). It is NOT tied to the cursor profile: a
    // humanized cursor should still type instantly - pacing a 66-char path to 20s is
    // pointless. Default falls through to Unicode input below.
    let slow = args.get("slow").and_then(Value::as_bool).unwrap_or(false);
    if slow && n > 0 {
        let lease = RefCell::new(TargetLease::new(
            || super::verify_expected_keyboard_target(args),
            focus_anchor,
        ));
        let failure: RefCell<Option<anyhow::Error>> = RefCell::new(None);
        let r = human_input::human_type(
            &text,
            profile,
            cancel,
            &|unit| {
                if failure.borrow().is_some() {
                    return false;
                }
                if let Err(error) = lease.borrow_mut().check() {
                    *failure.borrow_mut() = Some(error);
                    return false;
                }
                match super::send(&[super::key_unicode(unit, false)]) {
                    Ok(()) => {
                        started_units.set(started_units.get() + 1);
                        true
                    }
                    Err(error) => {
                        let _ = super::release(&[super::key_unicode(unit, true)]);
                        *failure.borrow_mut() = Some(error.into());
                        false
                    }
                }
            },
            &|unit| {
                if failure.borrow().is_some() {
                    return false;
                }
                if let Err(error) = lease.borrow_mut().check() {
                    let _ = super::release(&[super::key_unicode(unit, true)]);
                    *failure.borrow_mut() = Some(error);
                    return false;
                }
                match super::send(&[super::key_unicode(unit, true)]) {
                    Ok(()) => true,
                    Err(error) => {
                        let _ = super::release(&[super::key_unicode(unit, true)]);
                        *failure.borrow_mut() = Some(error.into());
                        false
                    }
                }
            },
        );
        if let Some(error) = failure.into_inner() {
            return Err(typing_error(error, started_units.get(), total_units));
        }
        if r == Outcome::Aborted {
            return Ok(aborted_typing(n, started_units.get(), total_units));
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_typing(n, started_units.get(), total_units));
        }
        if enter {
            send_key_tap(args, VK_RETURN)
                .map_err(|error| typing_error(error, started_units.get(), total_units))?;
        }
        return Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}));
    }
    let mut lease = TargetLease::new(
        || super::verify_expected_keyboard_target(args),
        focus_anchor,
    );
    if let Some(dispatched) = type_unicode_units(
        &text,
        cancel,
        |unit| send_unicode_unit(&mut || lease.check(), unit, &started_units),
        || sleep(Duration::from_millis(2)),
    )
    .map_err(|error| typing_error(error, started_units.get(), total_units))?
    {
        debug_assert_eq!(dispatched, started_units.get());
        return Ok(aborted_typing(n, started_units.get(), total_units));
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(aborted_typing(n, started_units.get(), total_units));
    }
    if enter {
        send_key_tap(args, VK_RETURN)
            .map_err(|error| typing_error(error, started_units.get(), total_units))?;
    }
    Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}))
}

fn send_unicode_unit(
    guard: &mut dyn FnMut() -> Result<()>,
    unit: u16,
    started_units: &Cell<usize>,
) -> Result<()> {
    let downs = [super::key_unicode(unit, false)];
    let ups = [super::key_unicode(unit, true)];
    dispatch_guarded_key_edges(
        guard,
        || {
            super::send(&downs)?;
            started_units.set(started_units.get() + 1);
            Ok(())
        },
        || (),
        || super::send(&ups).map_err(Into::into),
        || {
            let _ = super::release(&ups);
        },
    )
}

#[derive(Debug)]
pub(super) struct TypingProgress {
    pub started_units: usize,
    pub total_units: usize,
}

impl std::fmt::Display for TypingProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "text input failed after starting {} of {} UTF-16 units",
            self.started_units, self.total_units
        )
    }
}

fn typing_error(error: anyhow::Error, started_units: usize, total_units: usize) -> anyhow::Error {
    error.context(TypingProgress {
        started_units,
        total_units,
    })
}

// Never stage text through the shared clipboard: a paste can be processed after
// any attempted restoration, so neither clipboard identity nor user state can
// be preserved reliably for an asynchronous target.
fn type_unicode_units(
    text: &str,
    cancel: &AtomicBool,
    mut send_unit: impl FnMut(u16) -> Result<()>,
    mut pause: impl FnMut(),
) -> Result<Option<usize>> {
    for (dispatched, unit) in text.encode_utf16().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Some(dispatched));
        }
        send_unit(unit)?;
        pause();
    }
    Ok(None)
}

fn send_key_edges(args: &Value, downs: &[INPUT], ups: &[INPUT]) -> Result<()> {
    dispatch_guarded_key_edges(
        || super::verify_expected_keyboard_target(args),
        || super::send(downs).map_err(Into::into),
        || (),
        || super::send(ups).map_err(Into::into),
        || {
            let _ = super::release(ups);
        },
    )
}

pub(super) fn key_combination(args: &Value, cancel: &AtomicBool) -> Result<Value> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(super::aborted());
    }
    let combo = args
        .get("keys")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing keys"))?;
    let sequence = parse_key_sequence(combo)?;
    // HOLD the key(s) down before releasing. A game polls input each frame (~16ms
    // @60fps), so a 0-duration down->up tap is missed entirely - exactly why a key
    // "didn't move the character". Default a few frames so any tap lands;
    // `hold_seconds` holds longer for sustained movement (walk/run in a game).
    let hold_ms = args
        .get("hold_seconds")
        .and_then(Value::as_f64)
        .map(|s| (s.clamp(0.0, 10.0) * 1000.0) as u64)
        .unwrap_or(0)
        .max(45);
    let mut completed_groups = 0;
    for vks in &sequence {
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_keys(completed_groups));
        }
        if press_chord(args, vks, hold_ms, cancel)? {
            return Ok(aborted_keys(completed_groups));
        }
        completed_groups += 1;
        if completed_groups < sequence.len() && human_input::sleep_cancellable(20, cancel) {
            return Ok(aborted_keys(completed_groups));
        }
    }
    Ok(json!({
        "ok": true,
        "keys": combo,
        "held_ms": hold_ms,
        "sequence_groups": sequence.len(),
    }))
}

/// Press all keys down in order and release them in reverse so modifiers wrap
/// the primary key. Returns true when cancellation interrupted the hold.
fn press_chord(
    args: &Value,
    vks: &[VIRTUAL_KEY],
    hold_ms: u64,
    cancel: &AtomicBool,
) -> Result<bool> {
    let downs: Vec<INPUT> = vks.iter().map(|&vk| super::key_vk(vk, false)).collect();
    let ups: Vec<INPUT> = vks
        .iter()
        .rev()
        .map(|&vk| super::key_vk(vk, true))
        .collect();
    dispatch_guarded_key_edges(
        || super::verify_expected_keyboard_target(args),
        || super::send(&downs).map_err(Into::into),
        || human_input::sleep_cancellable(hold_ms, cancel),
        || super::send(&ups).map_err(Into::into),
        || {
            let _ = super::release(&ups);
        },
    )
}

/// Validate the original focus only at the final pre-dispatch edge. Key-down is
/// itself allowed to change focus, close a surface, submit, or switch windows;
/// requiring the old target to survive that requested effect creates a false
/// failure and can provoke a duplicate retry. Once key-down is attempted, key-up
/// is always attempted and then retried best-effort on any dispatch error.
fn dispatch_guarded_key_edges<T>(
    mut guard: impl FnMut() -> Result<()>,
    mut key_down: impl FnMut() -> Result<()>,
    between_edges: impl FnOnce() -> T,
    mut key_up: impl FnMut() -> Result<()>,
    mut emergency_release: impl FnMut(),
) -> Result<T> {
    guard()?;
    if let Err(error) = key_down() {
        emergency_release();
        return Err(error);
    }
    let outcome = between_edges();
    if let Err(error) = key_up() {
        emergency_release();
        return Err(error);
    }
    Ok(outcome)
}

fn send_key_tap(args: &Value, vk: VIRTUAL_KEY) -> Result<()> {
    send_key_edges(
        args,
        &[super::key_vk(vk, false)],
        &[super::key_vk(vk, true)],
    )
}

fn press_enter_requested(args: &Value) -> bool {
    args.get("press_enter")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn aborted_typing(requested_chars: usize, started_units: usize, total_units: usize) -> Value {
    json!({
        "ok": false,
        "status": "aborted_by_user",
        "requested_chars": requested_chars,
        "typed_partial": started_units > 0 && started_units < total_units,
        "started_utf16_units": started_units,
        "effect_may_have_occurred": started_units > 0,
    })
}

fn aborted_keys(completed_groups: usize) -> Value {
    json!({
        "ok": false,
        "status": "aborted_by_user",
        "completed_groups": completed_groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn text_content_never_implies_submission() {
        for text in ["hello{enter}", "hello{return}", "hello\n"] {
            assert!(!press_enter_requested(&json!({"text": text})));
        }
        assert!(press_enter_requested(
            &json!({"text": "hello\n", "press_enter": true})
        ));
    }

    #[test]
    fn long_unicode_text_is_dispatched_exactly_as_utf16() {
        let text = "A long input with tiếng Việt, 😀, and\nnewlines";
        let cancel = AtomicBool::new(false);
        let mut dispatched = Vec::new();
        let result = type_unicode_units(
            text,
            &cancel,
            |unit| {
                dispatched.push(unit);
                Ok(())
            },
            || {},
        )
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(dispatched, text.encode_utf16().collect::<Vec<_>>());
    }

    #[test]
    fn unicode_dispatch_stops_on_cancellation_or_failure() {
        let cancel = AtomicBool::new(false);
        let mut dispatched = Vec::new();
        let result = type_unicode_units(
            "long text with multiple units",
            &cancel,
            |unit| {
                dispatched.push(unit);
                cancel.store(true, Ordering::Relaxed);
                Ok(())
            },
            || {},
        )
        .unwrap();
        assert_eq!(result, Some(1));
        assert_eq!(dispatched, [b'l' as u16]);

        cancel.store(false, Ordering::Relaxed);
        let mut attempts = 0;
        let result = type_unicode_units(
            "multiple units",
            &cancel,
            |_| {
                attempts += 1;
                Err(anyhow!("dispatch failed"))
            },
            || panic!("failed dispatch must not continue"),
        );
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn slow_typing_cancelled_before_first_key_reports_no_effect() {
        let cancel = AtomicBool::new(true);
        let started = Cell::new(0);
        let outcome = human_input::human_type(
            "multiple units",
            &HumanProfile::instant(),
            &cancel,
            &|_| {
                started.set(started.get() + 1);
                true
            },
            &|_| panic!("no key was pressed"),
        );
        assert_eq!(outcome, Outcome::Aborted);
        let receipt = aborted_typing(14, started.get(), 14);
        assert_eq!(receipt["typed_partial"], false);
        assert_eq!(receipt["effect_may_have_occurred"], false);
        assert_eq!(receipt["started_utf16_units"], 0);
    }

    #[test]
    fn tab_focus_transition_after_key_down_still_releases() {
        let original_focus = Cell::new(true);
        let guard_calls = Cell::new(0);
        let released = Cell::new(false);

        dispatch_guarded_key_edges(
            || {
                guard_calls.set(guard_calls.get() + 1);
                anyhow::ensure!(original_focus.get(), "focus changed");
                Ok(())
            },
            || {
                original_focus.set(false);
                Ok(())
            },
            || (),
            || {
                released.set(true);
                Ok(())
            },
            || panic!("successful release must not use the emergency path"),
        )
        .unwrap();

        assert_eq!(guard_calls.get(), 1);
        assert!(released.get());
    }

    #[test]
    fn enter_target_disappearance_after_key_down_is_not_a_false_failure() {
        let target_exists = Cell::new(true);
        let released = Cell::new(false);

        dispatch_guarded_key_edges(
            || {
                anyhow::ensure!(target_exists.get(), "target disappeared");
                Ok(())
            },
            || {
                target_exists.set(false);
                Ok(())
            },
            || (),
            || {
                released.set(true);
                Ok(())
            },
            || panic!("successful release must not use the emergency path"),
        )
        .unwrap();

        assert!(!target_exists.get());
        assert!(released.get());
    }

    #[test]
    fn interrupted_transitioning_chord_releases_before_reporting_cancellation() {
        let original_window = Cell::new(true);
        let released = Cell::new(false);

        let interrupted = dispatch_guarded_key_edges(
            || {
                anyhow::ensure!(original_window.get(), "window changed");
                Ok(())
            },
            || {
                original_window.set(false);
                Ok(())
            },
            || true,
            || {
                released.set(true);
                Ok(())
            },
            || panic!("successful release must not use the emergency path"),
        )
        .unwrap();

        assert!(interrupted);
        assert!(released.get());
    }
}
