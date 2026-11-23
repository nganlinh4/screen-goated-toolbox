// --- SELECTION STATE ---
// Static variables, atomics, and constants for selection overlay.

use crate::win_types::SendHwnd;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::HHOOK;

// --- ABORT SIGNAL ---
pub static SELECTION_ABORT_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub(super) static CAPTURE_TARGET: LazyLock<
    Mutex<(
        crate::overlay::image_capture_target::ImageCaptureTarget,
        crate::overlay::process::chain::AutoPasteDestination,
    )>,
> = LazyLock::new(|| {
    Mutex::new((
        Default::default(),
        crate::overlay::process::chain::AutoPasteDestination::External(None),
    ))
});

// Whether this selection session holds a shared magnifier lease.
pub static MAG_LEASE_HELD: AtomicBool = AtomicBool::new(false);

// --- CONFIGURATION CONSTANTS ---
pub const FADE_TIMER_ID: usize = 2;
pub const TARGET_OPACITY: u8 = 120;
pub const FADE_IN_STEP: u8 = 40;
pub const FADE_OUT_STEP: u8 = 12;

// --- ZOOM CONSTANTS ---
pub const ZOOM_STEP: f32 = 0.25;
pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 4.0;
pub const ZOOM_TIMER_ID: usize = 3;
pub const CONTINUOUS_CHECK_TIMER_ID: usize = 4;

// --- MAIN STATE ---
pub static mut CURRENT_HOTKEY_ID: i32 = 0;
pub static mut START_POS: POINT = POINT { x: 0, y: 0 };
pub static mut CURR_POS: POINT = POINT { x: 0, y: 0 };
pub static mut IS_DRAGGING: bool = false;
pub static mut IS_SELECTION_COMMITTED: bool = false;
pub static mut IS_FADING_OUT: bool = false;
pub static mut CURRENT_ALPHA: u8 = 0;
pub static mut FADE_OUT_START_ALPHA: Option<u8> = None;
pub static SELECTION_OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static mut SELECTION_OVERLAY_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
pub static mut SELECTION_HOOK: HHOOK = HHOOK(std::ptr::null_mut());

// --- CONTINUOUS MODE HOTKEY TRACKING ---
/// Virtual key of the hotkey that opened the selection; 0 when untracked.
/// Written by the overlay thread and read by the keyboard hook.
pub static TRIGGER_VK_CODE: AtomicU32 = AtomicU32::new(0);
pub static IS_HOTKEY_HELD: AtomicBool = AtomicBool::new(false);
pub static CONTINUOUS_ACTIVATED_THIS_SESSION: AtomicBool = AtomicBool::new(false);
pub static HOLD_DETECTED_THIS_SESSION: AtomicBool = AtomicBool::new(false);

// --- CACHED BACK BUFFER ---
pub static CACHED_DIB: Mutex<crate::overlay::cached_dib::CachedDib> =
    Mutex::new(crate::overlay::cached_dib::CachedDib::empty());

// --- ZOOM STATE ---
pub static mut ZOOM_LEVEL: f32 = 1.0;
pub static mut ZOOM_CENTER_X: f32 = 0.0;
pub static mut ZOOM_CENTER_Y: f32 = 0.0;

// --- SMOOTH ZOOM STATE ---
pub static mut RENDER_ZOOM: f32 = 1.0;
pub static mut RENDER_CENTER_X: f32 = 0.0;
pub static mut RENDER_CENTER_Y: f32 = 0.0;

// --- PANNING STATE ---
pub static mut IS_RIGHT_DRAGGING: bool = false;
pub static mut LAST_PAN_POS: POINT = POINT { x: 0, y: 0 };

// --- ZOOM ALPHA OVERRIDE ---
pub static mut ZOOM_ALPHA_OVERRIDE: Option<u8> = None;

// --- HELPER FUNCTIONS ---

pub fn is_selection_overlay_active() -> bool {
    SELECTION_OVERLAY_ACTIVE.load(Ordering::SeqCst)
}

pub(super) fn capture_target() -> crate::overlay::image_capture_target::ImageCaptureTarget {
    capture_request().0
}

pub(super) fn capture_request() -> (
    crate::overlay::image_capture_target::ImageCaptureTarget,
    crate::overlay::process::chain::AutoPasteDestination,
) {
    CAPTURE_TARGET.lock().map(|target| *target).unwrap_or((
        Default::default(),
        crate::overlay::process::chain::AutoPasteDestination::External(None),
    ))
}
