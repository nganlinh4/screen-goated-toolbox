// --- SELECTION STATE ---
// Static variables, atomics, and constants for selection overlay.

use crate::win_types::{SendHbitmap, SendHwnd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::UI::WindowsAndMessaging::HHOOK;
use windows_core::BOOL;

// --- ABORT SIGNAL ---
lazy_static::lazy_static! {
    pub static ref SELECTION_ABORT_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

// --- MAGNIFICATION API FFI ---
pub type MagInitializeFn = unsafe extern "system" fn() -> BOOL;
pub type MagUninitializeFn = unsafe extern "system" fn() -> BOOL;
pub type MagSetFullscreenTransformFn = unsafe extern "system" fn(f32, i32, i32) -> BOOL;

pub static mut MAG_DLL: HMODULE = HMODULE(std::ptr::null_mut());
pub static mut MAG_INITIALIZE: Option<MagInitializeFn> = None;
pub static mut MAG_UNINITIALIZE: Option<MagUninitializeFn> = None;
pub static mut MAG_SET_FULLSCREEN_TRANSFORM: Option<MagSetFullscreenTransformFn> = None;
pub static mut MAG_INITIALIZED: bool = false;

// --- CONFIGURATION CONSTANTS ---
pub const FADE_TIMER_ID: usize = 2;
pub const TARGET_OPACITY: u8 = 120;
pub const FADE_STEP: u8 = 40;

// --- ZOOM CONSTANTS ---
pub const ZOOM_STEP: f32 = 0.25;
pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 4.0;
pub const ZOOM_TIMER_ID: usize = 3;
pub const CONTINUOUS_CHECK_TIMER_ID: usize = 4;

// --- MAIN STATE ---
pub static mut CURRENT_PRESET_IDX: usize = 0;
pub static mut CURRENT_HOTKEY_ID: i32 = 0;
pub static mut START_POS: POINT = POINT { x: 0, y: 0 };
pub static mut CURR_POS: POINT = POINT { x: 0, y: 0 };
pub static mut IS_DRAGGING: bool = false;
pub static mut IS_FADING_OUT: bool = false;
pub static mut CURRENT_ALPHA: u8 = 0;
pub static SELECTION_OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static mut SELECTION_OVERLAY_HWND: SendHwnd = SendHwnd(HWND(std::ptr::null_mut()));
pub static mut SELECTION_HOOK: HHOOK = HHOOK(std::ptr::null_mut());

// --- CONTINUOUS MODE HOTKEY TRACKING ---
pub static mut TRIGGER_VK_CODE: u32 = 0;
pub static mut TRIGGER_MODIFIERS: u32 = 0;
pub static IS_HOTKEY_HELD: AtomicBool = AtomicBool::new(false);
pub static CONTINUOUS_ACTIVATED_THIS_SESSION: AtomicBool = AtomicBool::new(false);
pub static HOLD_DETECTED_THIS_SESSION: AtomicBool = AtomicBool::new(false);

// --- CACHED BACK BUFFER ---
pub static mut CACHED_BITMAP: SendHbitmap = SendHbitmap(HBITMAP(std::ptr::null_mut()));
pub static mut CACHED_BITS: *mut u8 = std::ptr::null_mut();
pub static mut CACHED_W: i32 = 0;
pub static mut CACHED_H: i32 = 0;

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
