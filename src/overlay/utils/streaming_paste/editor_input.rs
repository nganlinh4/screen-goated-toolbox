const INSERTION_TAG: usize = 0x53475450;
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

pub(super) fn input_settled(state: &mut Option<Instant>) -> bool {
    state.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(20)
}

pub(super) fn shortcut_held() -> bool {
    [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } < 0)
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
