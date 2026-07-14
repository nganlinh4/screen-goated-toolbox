//! Keyboard action handlers for the Computer Control executor: `type_text`
//! (clipboard-paste fast path, slow human typing, or instant Unicode batch) and
//! `key_combination` (named-key / chord parsing). The raw `SendInput` key builders
//! live in the parent module and are reached via `super::`.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F2,
    VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_HOME, VK_LEFT,
    VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};

use super::super::human_input::{self, HumanProfile, Outcome};

pub(super) fn type_text(
    args: &Value,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Result<Value> {
    super::super::uia::focus_foreground(); // text must land on the on-screen window
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
    super::verify_expected_keyboard_target(args)?;
    // PASTE longer text via the clipboard instead of a keystroke per character
    // (slow for paragraphs, mangles non-ASCII). Save/restore the user's clipboard.
    // Short inputs still type, to leave the clipboard alone and play nice with
    // type-as-you-search fields.
    let saved = super::super::clipboard::get_text();
    // Only take the paste fast-path when the clipboard holds NO non-text data we
    // can't restore (an image, copied files). If any such format is present we type
    // instead - even when text ALSO exists, because re-setting our saved text would
    // EmptyClipboard the rich formats and silently downgrade them to plain text.
    let would_clobber = super::super::clipboard::has_nontext();
    if n > 12 && !would_clobber {
        super::super::clipboard::set_text(&text);
        let cancelled_before_paste = human_input::sleep_cancellable(30, cancel);
        let paste_result = if cancelled_before_paste {
            Ok(())
        } else {
            send_ctrl_v(args)
        };
        let cancelled_after_paste = if paste_result.is_ok() && !cancelled_before_paste {
            human_input::sleep_cancellable(140, cancel)
        } else {
            false
        };
        restore_clipboard(&saved);
        paste_result?;
        if cancelled_before_paste {
            return Ok(super::aborted());
        }
        if cancelled_after_paste {
            return Ok(aborted_typing(n, false));
        }
        if enter {
            send_key_tap(args, VK_RETURN)?;
        }
        return Ok(json!({"ok": true, "typed_chars": n, "method": "paste", "submitted": enter}));
    }
    // Slow, human-paced per-key typing ONLY when explicitly asked for (a rare field
    // that demands paced keystrokes). It is NOT tied to the cursor profile: a
    // humanized cursor should still type instantly - pacing a 66-char path to 20s is
    // pointless. Default falls through to the instant batch below.
    let slow = args.get("slow").and_then(Value::as_bool).unwrap_or(false);
    if slow && n > 0 {
        super::verify_expected_keyboard_target(args)?;
        let failure: RefCell<Option<anyhow::Error>> = RefCell::new(None);
        let r = human_input::human_type(
            &text,
            profile,
            cancel,
            &|unit| {
                if failure.borrow().is_some() {
                    return false;
                }
                if let Err(error) = super::verify_expected_keyboard_target(args) {
                    *failure.borrow_mut() = Some(error);
                    return false;
                }
                match super::send(&[super::key_unicode(unit, false)]) {
                    Ok(()) => true,
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
                if let Err(error) = super::verify_expected_keyboard_target(args) {
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
            return Err(error);
        }
        if r == Outcome::Aborted {
            return Ok(aborted_typing(n, true));
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_typing(n, false));
        }
        if enter {
            send_key_tap(args, VK_RETURN)?;
        }
        return Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}));
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    for unit in units {
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_typing(n, true));
        }
        send_key_edges(
            args,
            &[super::key_unicode(unit, false)],
            &[super::key_unicode(unit, true)],
        )?;
        sleep(Duration::from_millis(2));
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(aborted_typing(n, false));
    }
    if enter {
        send_key_tap(args, VK_RETURN)?;
    }
    Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}))
}

/// Press Ctrl+V (paste) — Ctrl down, V down, V up, Ctrl up.
fn send_ctrl_v(args: &Value) -> Result<()> {
    let v = VIRTUAL_KEY(0x56); // 'V'
    let downs = [super::key_vk(VK_CONTROL, false), super::key_vk(v, false)];
    let ups = [super::key_vk(v, true), super::key_vk(VK_CONTROL, true)];
    send_key_edges(args, &downs, &ups)
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
    super::super::uia::focus_foreground(); // keys must land on the on-screen window
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

fn parse_key_sequence(combo: &str) -> Result<Vec<Vec<VIRTUAL_KEY>>> {
    if combo.trim().is_empty() {
        return Err(anyhow!("empty key combination"));
    }
    combo
        .split(',')
        .enumerate()
        .map(|(group_index, group)| {
            if group.trim().is_empty() {
                return Err(anyhow!("empty key group at position {}", group_index + 1));
            }
            group
                .split('+')
                .map(str::trim)
                .map(|token| {
                    if token.is_empty() {
                        Err(anyhow!("empty key in group {}", group_index + 1))
                    } else {
                        token_to_vk(token).ok_or_else(|| anyhow!("unknown key: {token}"))
                    }
                })
                .collect()
        })
        .collect()
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

fn restore_clipboard(saved: &str) {
    if saved.is_empty() {
        super::super::clipboard::clear();
    } else {
        super::super::clipboard::set_text(saved);
    }
}

fn press_enter_requested(args: &Value) -> bool {
    args.get("press_enter")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn aborted_typing(requested_chars: usize, partial: bool) -> Value {
    json!({
        "ok": false,
        "status": "aborted_by_user",
        "requested_chars": requested_chars,
        "typed_partial": partial,
    })
}

fn aborted_keys(completed_groups: usize) -> Value {
    json!({
        "ok": false,
        "status": "aborted_by_user",
        "completed_groups": completed_groups,
    })
}

fn token_to_vk(token: &str) -> Option<VIRTUAL_KEY> {
    let lower = token.to_ascii_lowercase();
    let vk = match lower.as_str() {
        "ctrl" | "control" => VK_CONTROL,
        "alt" | "menu" => VK_MENU,
        "shift" => VK_SHIFT,
        "win" | "super" | "meta" | "cmd" => VK_LWIN,
        "enter" | "return" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" | "escape" => VK_ESCAPE,
        "space" | "spacebar" => VK_SPACE,
        "backspace" | "back" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" | "pgup" => VK_PRIOR,
        "pagedown" | "pgdn" => VK_NEXT,
        "f1" => VK_F1,
        "f2" => VK_F2,
        "f3" => VK_F3,
        "f4" => VK_F4,
        "f5" => VK_F5,
        "f6" => VK_F6,
        "f7" => VK_F7,
        "f8" => VK_F8,
        "f9" => VK_F9,
        "f10" => VK_F10,
        "f11" => VK_F11,
        "f12" => VK_F12,
        _ => {
            let bytes = lower.as_bytes();
            if bytes.len() == 1 {
                let c = bytes[0];
                if c.is_ascii_lowercase() {
                    return Some(VIRTUAL_KEY((c.to_ascii_uppercase()) as u16));
                }
                if c.is_ascii_digit() {
                    return Some(VIRTUAL_KEY(c as u16));
                }
            }
            return None;
        }
    };
    Some(vk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn maps_letter_and_named_keys() {
        assert_eq!(token_to_vk("c"), Some(VIRTUAL_KEY(b'C' as u16)));
        assert_eq!(token_to_vk("Ctrl"), Some(VK_CONTROL));
        assert_eq!(token_to_vk("enter"), Some(VK_RETURN));
        assert_eq!(token_to_vk("F5"), Some(VK_F5));
        assert!(token_to_vk("nope").is_none());
    }

    #[test]
    fn parses_sequential_chords_without_changing_plus_semantics() {
        let sequence = parse_key_sequence("Ctrl+Shift+K, Tab").unwrap();
        assert_eq!(sequence.len(), 2);
        assert_eq!(
            sequence[0],
            [VK_CONTROL, VK_SHIFT, VIRTUAL_KEY(b'K' as u16)]
        );
        assert_eq!(sequence[1], [VK_TAB]);
    }

    #[test]
    fn rejects_empty_sequence_groups() {
        let error = parse_key_sequence("A, ,B").unwrap_err();
        assert!(error.to_string().contains("empty key group"));
    }

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
