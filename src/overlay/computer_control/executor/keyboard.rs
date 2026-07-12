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
    // Submit handling: models routinely append a submit token ("…{enter}", a
    // trailing newline) or pass press_enter. Honor all of them — type the literal
    // text, then press Enter instead of typing the submit marker verbatim.
    let mut text = raw.to_string();
    let mut enter = args
        .get("press_enter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let lower = text.to_lowercase();
    for tok in ["{enter}", "{return}", "\n"] {
        if lower.ends_with(tok) {
            text.truncate(text.len() - tok.len());
            enter = true;
            break;
        }
    }
    let n = text.chars().count();
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
            send_ctrl_v()
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
            send_key_tap(VK_RETURN)?;
        }
        return Ok(json!({"ok": true, "typed_chars": n, "method": "paste", "submitted": enter}));
    }
    // Slow, human-paced per-key typing ONLY when explicitly asked for (a rare field
    // that demands paced keystrokes). It is NOT tied to the cursor profile: a
    // humanized cursor should still type instantly - pacing a 66-char path to 20s is
    // pointless. Default falls through to the instant batch below.
    let slow = args.get("slow").and_then(Value::as_bool).unwrap_or(false);
    if slow && n > 0 {
        let failure = RefCell::new(None);
        let r = human_input::human_type(
            &text,
            profile,
            cancel,
            &|unit| {
                if failure.borrow().is_some() {
                    return false;
                }
                match super::send(&[super::key_unicode(unit, false)]) {
                    Ok(()) => true,
                    Err(error) => {
                        let _ = super::release(&[super::key_unicode(unit, true)]);
                        *failure.borrow_mut() = Some(error);
                        false
                    }
                }
            },
            &|unit| {
                if failure.borrow().is_some() {
                    return false;
                }
                match super::send(&[super::key_unicode(unit, true)]) {
                    Ok(()) => true,
                    Err(error) => {
                        let _ = super::release(&[super::key_unicode(unit, true)]);
                        *failure.borrow_mut() = Some(error);
                        false
                    }
                }
            },
        );
        if let Some(error) = failure.into_inner() {
            return Err(error.into());
        }
        if r == Outcome::Aborted {
            return Ok(aborted_typing(n, true));
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_typing(n, false));
        }
        if enter {
            send_key_tap(VK_RETURN)?;
        }
        return Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}));
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    // Send in chunks so very long strings don't overflow a single call.
    for chunk in units.chunks(32) {
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_typing(n, true));
        }
        let mut inputs = Vec::with_capacity(chunk.len() * 2);
        for &unit in chunk {
            inputs.push(super::key_unicode(unit, false));
            inputs.push(super::key_unicode(unit, true));
        }
        if let Err(error) = super::send(&inputs) {
            let releases: Vec<INPUT> = chunk
                .iter()
                .rev()
                .map(|&unit| super::key_unicode(unit, true))
                .collect();
            let _ = super::release(&releases);
            return Err(error.into());
        }
        sleep(Duration::from_millis(2));
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(aborted_typing(n, false));
    }
    if enter {
        send_key_tap(VK_RETURN)?;
    }
    Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}))
}

/// Press Ctrl+V (paste) — Ctrl down, V down, V up, Ctrl up.
fn send_ctrl_v() -> Result<()> {
    let v = VIRTUAL_KEY(0x56); // 'V'
    let inputs = [
        super::key_vk(VK_CONTROL, false),
        super::key_vk(v, false),
        super::key_vk(v, true),
        super::key_vk(VK_CONTROL, true),
    ];
    if let Err(error) = super::send(&inputs) {
        let _ = super::release(&[super::key_vk(v, true), super::key_vk(VK_CONTROL, true)]);
        return Err(error.into());
    }
    Ok(())
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
        if press_chord(vks, hold_ms, cancel)? {
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
fn press_chord(vks: &[VIRTUAL_KEY], hold_ms: u64, cancel: &AtomicBool) -> Result<bool> {
    let downs: Vec<INPUT> = vks.iter().map(|&vk| super::key_vk(vk, false)).collect();
    let ups: Vec<INPUT> = vks
        .iter()
        .rev()
        .map(|&vk| super::key_vk(vk, true))
        .collect();
    if let Err(error) = super::send(&downs) {
        let _ = super::release(&ups);
        return Err(error.into());
    }
    let interrupted = human_input::sleep_cancellable(hold_ms, cancel);
    if let Err(error) = super::send(&ups) {
        let _ = super::release(&ups);
        return Err(error.into());
    }
    Ok(interrupted)
}

fn send_key_tap(vk: VIRTUAL_KEY) -> Result<()> {
    let up = super::key_vk(vk, true);
    if let Err(error) = super::send(&[super::key_vk(vk, false)]) {
        let _ = super::release(std::slice::from_ref(&up));
        return Err(error.into());
    }
    if let Err(error) = super::send(std::slice::from_ref(&up)) {
        let _ = super::release(std::slice::from_ref(&up));
        return Err(error.into());
    }
    Ok(())
}

fn restore_clipboard(saved: &str) {
    if saved.is_empty() {
        super::super::clipboard::clear();
    } else {
        super::super::clipboard::set_text(saved);
    }
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
}
