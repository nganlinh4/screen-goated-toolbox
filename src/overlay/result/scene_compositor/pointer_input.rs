//! Physical press lifetime starts before asynchronous renderer gesture recognition.
use std::sync::Mutex;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture,
};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy)]
struct Press {
    hwnd: isize,
    button: u32,
    origin: POINT,
    point: POINT,
    released: bool,
    claimed: bool,
    pending: bool,
}

static PRESS: Mutex<Option<Press>> = Mutex::new(None);
static HOOK: Mutex<isize> = Mutex::new(0);
pub(super) const WM_APP_POINTER_DISPATCH: u32 = WM_APP + 43;

fn up_message(button: u32) -> u32 {
    match button {
        2 => WM_RBUTTONUP,
        4 => WM_MBUTTONUP,
        _ => WM_LBUTTONUP,
    }
}

pub(super) fn route(hwnd: HWND, message: u32, mut point: POINT) -> bool {
    if !unsafe { ClientToScreen(hwnd, &mut point) }.as_bool() {
        return false;
    }
    let button = match message {
        WM_LBUTTONDOWN => Some(1),
        WM_RBUTTONDOWN => Some(2),
        WM_MBUTTONDOWN => Some(4),
        _ => None,
    };
    if let Some(button) = button {
        cancel();
        *PRESS.lock().unwrap() = Some(Press {
            hwnd: hwnd.0 as isize,
            button,
            origin: point,
            point,
            released: false,
            claimed: false,
            pending: false,
        });
        unsafe {
            SetCapture(hwnd);
            if let Ok(hook) = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None::<HINSTANCE>, 0)
            {
                *HOOK.lock().unwrap() = hook.0 as isize;
            }
        }
        return false;
    }
    let press = *PRESS.lock().unwrap();
    let Some(press) = press else { return false };
    if message != WM_MOUSEMOVE && message != up_message(press.button) {
        return false;
    }
    if press.released {
        return message == up_message(press.button);
    }
    if *HOOK.lock().unwrap() == 0 {
        observe(message, point);
    }
    true
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        observe(wparam.0 as u32, event.pt);
    }
    unsafe { CallNextHookEx(None::<HHOOK>, code, wparam, lparam) }
}

fn observe(message: u32, point: POINT) {
    let Ok(mut state) = PRESS.try_lock() else {
        return;
    };
    let Some(press) = state.as_mut() else { return };
    if press.released || (message != WM_MOUSEMOVE && message != up_message(press.button)) {
        return;
    }
    press.point = point;
    press.released = message != WM_MOUSEMOVE;
    if !press.pending {
        press.pending = unsafe {
            PostMessageW(
                Some(HWND(press.hwnd as *mut _)),
                WM_APP_POINTER_DISPATCH,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .is_ok();
    }
}

pub(super) fn dispatch() {
    let press = {
        let mut state = PRESS.lock().unwrap();
        let Some(press) = state.as_mut().filter(|press| press.pending) else {
            return;
        };
        press.pending = false;
        *press
    };
    let hwnd = HWND(press.hwnd as *mut _);
    super::input_surface::forward_observed_mouse(hwnd, press.point, press.button, WM_MOUSEMOVE);
    super::gesture::observe_mouse_event(WM_MOUSEMOVE, press.point);
    if press.released {
        let message = up_message(press.button);
        super::input_surface::forward_observed_mouse(hwnd, press.point, press.button, message);
        super::gesture::observe_mouse_event(message, press.point);
        release(press.hwnd);
    }
}

pub(super) fn claim(button: u32, hwnd: HWND, mut origin: POINT) -> Option<POINT> {
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return None;
    }
    let mut state = PRESS.lock().unwrap();
    let press = state.as_mut()?;
    if press.claimed
        || press.button != button
        || press.hwnd != hwnd.0 as isize
        || press.origin.x.abs_diff(origin.x) > 1
        || press.origin.y.abs_diff(origin.y) > 1
    {
        return None;
    }
    press.claimed = true;
    Some(press.origin)
}

pub(super) fn replay() {
    let press = *PRESS.lock().unwrap();
    if let Some(press) = press.filter(|press| press.claimed) {
        super::gesture::observe_mouse_event(
            if press.released {
                up_message(press.button)
            } else {
                WM_MOUSEMOVE
            },
            press.point,
        );
    }
}

fn release(hwnd: isize) {
    let hook = std::mem::take(&mut *HOOK.lock().unwrap());
    unsafe {
        if hook != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(hook as *mut _));
        }
        if hwnd != 0 && GetCapture().0 as isize == hwnd {
            let _ = ReleaseCapture();
        }
    }
}

pub(super) fn cancel() {
    let press = PRESS.lock().unwrap().take();
    release(press.map_or(0, |press| press.hwnd));
    if press.is_some() {
        super::gesture::on_capture_lost();
    }
}

pub(super) fn capture_lost() {
    if PRESS
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|press| !press.released)
    {
        cancel();
    }
}

pub(super) fn reconcile() {
    dispatch();
    let missed_release = PRESS.lock().unwrap().as_ref().is_some_and(|press| {
        !press.released && unsafe { GetAsyncKeyState(press.button as i32) } >= 0
    });
    if missed_release {
        cancel();
    }
}
