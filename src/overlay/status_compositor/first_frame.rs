//! Hold the first native reveal until the browser has painted its scene.

use super::protocol::{RecordingScene, StatusSnapshot};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos, ShowWindow,
};

static PAINTED: AtomicBool = AtomicBool::new(false);
static REQUESTED: AtomicBool = AtomicBool::new(false);
static LATEST_REQUEST: AtomicU64 = AtomicU64::new(0);

pub(super) fn painted() -> bool {
    PAINTED.load(Ordering::SeqCst)
}

pub(super) fn request(scene: &StatusSnapshot) -> Option<u64> {
    if !super::region::has_visible_content(scene) {
        REQUESTED.store(false, Ordering::SeqCst);
        LATEST_REQUEST.fetch_add(1, Ordering::SeqCst);
        return None;
    }
    if painted() || REQUESTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    Some(LATEST_REQUEST.fetch_add(1, Ordering::SeqCst) + 1)
}

pub(super) fn retry() {
    REQUESTED.store(false, Ordering::SeqCst);
}

pub(super) fn acknowledge(request: u64) -> bool {
    if request != LATEST_REQUEST.load(Ordering::SeqCst) {
        return false;
    }
    !PAINTED.swap(true, Ordering::SeqCst)
}

pub(super) fn sync_input_window(recording: Option<&RecordingScene>) {
    let value = super::child::INPUT_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }
    let hwnd = HWND(value as *mut std::ffi::c_void);
    unsafe {
        if let Some(recording) = recording.filter(|recording| recording.visible && painted()) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                recording.rect.x,
                recording.rect.y,
                recording.rect.width.max(1),
                recording.rect.height.max(1),
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        } else {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}
