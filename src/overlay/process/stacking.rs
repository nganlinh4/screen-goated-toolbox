//! Coalesced requests delivered only to this host's live glow controllers.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub(super) const WM_RECONCILE_STACK: u32 = WM_APP + 94;
static CONTROLLERS: LazyLock<Mutex<HashMap<isize, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn register(hwnd: HWND) {
    CONTROLLERS.lock().unwrap().insert(hwnd.0 as isize, false);
}

pub(super) fn unregister(hwnd: HWND) {
    CONTROLLERS.lock().unwrap().remove(&(hwnd.0 as isize));
}

pub(super) fn request() {
    // Keep this registry independent of rendering state: a busy paint must not
    // block the renderer event reader. PostMessage never waits for the owner.
    for (id, pending) in CONTROLLERS.lock().unwrap().iter_mut() {
        if !*pending {
            *pending = unsafe {
                PostMessageW(
                    Some(HWND(*id as *mut _)),
                    WM_RECONCILE_STACK,
                    WPARAM(0),
                    LPARAM(0),
                )
            }
            .is_ok();
        }
    }
}

pub(super) fn take(hwnd: HWND) -> bool {
    CONTROLLERS
        .lock()
        .unwrap()
        .get_mut(&(hwnd.0 as isize))
        .is_some_and(std::mem::take)
}

#[cfg(test)]
#[path = "stacking_tests.rs"]
mod tests;
