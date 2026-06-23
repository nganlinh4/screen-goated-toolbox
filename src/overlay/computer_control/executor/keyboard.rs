//! Keyboard action handlers for the Computer Control executor: `type_text`
//! (clipboard-paste fast path, slow human typing, or instant Unicode batch) and
//! `key_combination` (named-key / chord parsing). The raw `SendInput` key builders
//! live in the parent module and are reached via `super::`.

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

pub(super) fn type_text(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    super::super::uia::focus_foreground(); // text must land on the on-screen window
    let raw = args.get("text").and_then(Value::as_str).ok_or_else(|| anyhow!("missing text"))?;
    // Submit handling: models routinely append a submit token ("…{enter}", a
    // trailing newline) or pass press_enter. Honor all of them — type the literal
    // text, then press Enter. Without this the field just gets
    // "chrome://extensions{enter}" typed verbatim (what stalled browser setup).
    let mut text = raw.to_string();
    let mut enter = args.get("press_enter").and_then(Value::as_bool).unwrap_or(false);
    let lower = text.to_lowercase();
    for tok in ["{enter}", "{return}", "\n"] {
        if lower.ends_with(tok) {
            text.truncate(text.len() - tok.len());
            enter = true;
            break;
        }
    }
    let n = text.chars().count();
    let press_enter = || {
        if enter && let Some(vk) = token_to_vk("enter") {
            super::send(&[super::key_vk(vk, false), super::key_vk(vk, true)]);
        }
    };

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
        sleep(Duration::from_millis(30));
        send_ctrl_v();
        sleep(Duration::from_millis(140));
        if saved.is_empty() {
            super::super::clipboard::clear(); // don't leave our text on a previously-empty clipboard
        } else {
            super::super::clipboard::set_text(&saved);
        }
        press_enter();
        return Ok(json!({"ok": true, "typed_chars": n, "method": "paste", "submitted": enter}));
    }
    // Slow, human-paced per-key typing ONLY when explicitly asked for (a rare field
    // that demands paced keystrokes). It is NOT tied to the cursor profile: a
    // humanized cursor should still type instantly - pacing a 66-char path to 20s is
    // pointless. Default falls through to the instant batch below.
    let slow = args.get("slow").and_then(Value::as_bool).unwrap_or(false);
    if slow && n > 0 {
        let r = human_input::human_type(
            &text,
            profile,
            cancel,
            &|unit| super::send(&[super::key_unicode(unit, false)]),
            &|unit| super::send(&[super::key_unicode(unit, true)]),
        );
        if r == Outcome::Aborted {
            return Ok(json!({"ok": false, "status": "aborted_by_user", "typed_partial": true}));
        }
        press_enter();
        return Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}));
    }
    let mut inputs: Vec<INPUT> = Vec::new();
    for unit in text.encode_utf16() {
        inputs.push(super::key_unicode(unit, false));
        inputs.push(super::key_unicode(unit, true));
    }
    // Send in chunks so very long strings don't overflow a single call.
    for chunk in inputs.chunks(64) {
        super::send(chunk);
        sleep(Duration::from_millis(2));
    }
    press_enter();
    Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}))
}

/// Press Ctrl+V (paste) — Ctrl down, V down, V up, Ctrl up.
fn send_ctrl_v() {
    let v = VIRTUAL_KEY(0x56); // 'V'
    super::send(&[
        super::key_vk(VK_CONTROL, false),
        super::key_vk(v, false),
        super::key_vk(v, true),
        super::key_vk(VK_CONTROL, true),
    ]);
}

pub(super) fn key_combination(args: &Value, cancel: &AtomicBool) -> Result<Value> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(super::aborted());
    }
    super::super::uia::focus_foreground(); // keys must land on the on-screen window
    let combo = args.get("keys").and_then(Value::as_str).ok_or_else(|| anyhow!("missing keys"))?;
    let mut vks = Vec::new();
    for token in combo.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        vks.push(token_to_vk(token).ok_or_else(|| anyhow!("unknown key: {token}"))?);
    }
    if vks.is_empty() {
        return Err(anyhow!("empty key combination"));
    }
    // Press all down in order, release in reverse (so modifiers wrap the key).
    let mut inputs: Vec<INPUT> = vks.iter().map(|&vk| super::key_vk(vk, false)).collect();
    inputs.extend(vks.iter().rev().map(|&vk| super::key_vk(vk, true)));
    super::send(&inputs);
    Ok(json!({"ok": true, "keys": combo}))
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
}
