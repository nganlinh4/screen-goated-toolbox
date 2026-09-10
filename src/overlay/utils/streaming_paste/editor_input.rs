use super::super::input_activity::{INSERTION_TAG, epoch};
use super::MAX_INPUT_UNITS;
use anyhow::{Result, ensure};
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub(super) fn validate_input(text: &str) -> Result<()> {
    ensure!(
        !text.chars().any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')),
        "control characters are not text input"
    );
    ensure!(
        text.encode_utf16().count() <= MAX_INPUT_UNITS,
        "input batch exceeds bound"
    );
    Ok(())
}

pub(super) fn no_held_modifiers() -> Result<()> {
    for key in [
        VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN, VK_LBUTTON, VK_RBUTTON, VK_MBUTTON,
    ] {
        ensure!(
            unsafe { GetAsyncKeyState(key.0 as i32) } >= 0,
            "keyboard modifier or mouse button is held"
        );
    }
    Ok(())
}

pub(super) fn input_epoch() -> u64 {
    epoch()
}

pub(super) fn input_settled(state: &mut Option<(u64, Instant)>) -> bool {
    let tick = input_epoch();
    if let Some((previous, since)) = state
        && *previous == tick
    {
        return since.elapsed() >= Duration::from_millis(20);
    }
    *state = Some((tick, Instant::now()));
    false
}

pub(super) fn send_unicode(text: &str, selected_deletion: bool) -> Result<()> {
    send_edit(text, selected_deletion, 0)
}

pub(super) fn send_edit(text: &str, selected_deletion: bool, backspaces: usize) -> Result<()> {
    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2 + 2);
    for _ in 0..backspaces {
        for flags in [KEYBD_EVENT_FLAGS(0), KEYEVENTF_KEYUP] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_BACK,
                        dwFlags: flags,
                        dwExtraInfo: INSERTION_TAG,
                        ..Default::default()
                    },
                },
            });
        }
    }
    for unit in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: unit,
                        dwFlags: flags,
                        dwExtraInfo: INSERTION_TAG,
                        ..Default::default()
                    },
                },
            });
        }
    }
    if inputs.is_empty() && selected_deletion {
        for flags in [KEYBD_EVENT_FLAGS(0), KEYEVENTF_KEYUP] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_DELETE,
                        dwFlags: flags,
                        dwExtraInfo: INSERTION_TAG,
                        ..Default::default()
                    },
                },
            });
        }
    }
    if !inputs.is_empty() {
        let inserted = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        ensure!(
            inserted as usize == inputs.len(),
            "Windows rejected or partially inserted the input batch"
        );
    }
    Ok(())
}
