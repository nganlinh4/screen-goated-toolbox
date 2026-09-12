//! Capture the native VK before egui merges numpad and main-row keys.

use std::cell::RefCell;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, WM_KEYDOWN, WM_KILLFOCUS, WM_NCDESTROY, WM_SYSKEYDOWN,
};

use crate::hotkey::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, names};

const SUBCLASS_ID: usize = 0x53475448;

#[derive(Default)]
struct Capture {
    owner: Option<usize>,
    pending: Option<(u32, u32)>,
    context: Option<eframe::egui::Context>,
}

thread_local! {
    static CAPTURE: RefCell<Capture> = RefCell::new(Capture::default());
}

pub(super) fn sync(hwnd: HWND, owner: Option<usize>, context: &eframe::egui::Context) {
    if hwnd.is_invalid() {
        return;
    }
    CAPTURE.with_borrow_mut(|capture| {
        if capture.owner != owner {
            capture.pending = None;
        }
        capture.owner = owner;
        capture.context = owner.map(|_| context.clone());
    });
    unsafe {
        if owner.is_some() {
            if !SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0).as_bool() {
                crate::log_info!("[Hotkey] native capture subclass unavailable");
            }
        } else {
            let _ = RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
        }
    }
}

pub(super) fn take() -> Option<(u32, u32)> {
    CAPTURE.with_borrow_mut(|capture| capture.pending.take())
}

fn modifiers() -> u32 {
    let down = |vk| unsafe { GetKeyState(vk) < 0 };
    (if down(0x11) { MOD_CONTROL } else { 0 })
        | (if down(0x12) { MOD_ALT } else { 0 })
        | (if down(0x10) { MOD_SHIFT } else { 0 })
        | (if down(0x5B) || down(0x5C) { MOD_WIN } else { 0 })
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN) && unsafe { GetForegroundWindow() } == hwnd {
        let code = wparam.0 as u32;
        let modifiers = modifiers();
        if !names::is_modifier(code) && !(code == 0x1B && modifiers == 0) {
            CAPTURE.with_borrow_mut(|capture| {
                if capture.owner.is_some() && lparam.0 & (1 << 30) == 0 {
                    capture.pending.get_or_insert((code, modifiers));
                    if let Some(context) = &capture.context {
                        context.request_repaint();
                    }
                }
            });
            return LRESULT(0);
        }
    } else if matches!(message, WM_KILLFOCUS | WM_NCDESTROY) {
        CAPTURE.with_borrow_mut(|capture| capture.pending = None);
        if message == WM_NCDESTROY {
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass), id);
            }
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
