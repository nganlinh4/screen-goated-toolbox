// --- TEXT SELECTION STATE ---
// Shared state, atomics, and constants for text selection badge.

use std::sync::{
    atomic::{AtomicBool, AtomicIsize, Ordering},
    Arc, Mutex, Once,
};
use windows::Win32::UI::WindowsAndMessaging::*;

// --- SHARED STATE ---
pub struct TextSelectionState {
    pub preset_idx: usize,
    pub is_selecting: bool,
    pub is_processing: bool,
    pub hook_handle: HHOOK,
    pub webview: Option<wry::WebView>,
}
unsafe impl Send for TextSelectionState {}

pub static SELECTION_STATE: Mutex<TextSelectionState> = Mutex::new(TextSelectionState {
    preset_idx: usize::MAX,
    is_selecting: false,
    is_processing: false,
    hook_handle: HHOOK(std::ptr::null_mut()),
    webview: None,
});

pub static REGISTER_TAG_CLASS: Once = Once::new();

lazy_static::lazy_static! {
    pub static ref TAG_ABORT_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref INITIAL_TEXT_GLOBAL: Mutex<String> = Mutex::new(String::from("Select text..."));
}

thread_local! {
    pub static SELECTION_WEB_CONTEXT: std::cell::RefCell<Option<wry::WebContext>> = const { std::cell::RefCell::new(None) };
}

// Warmup / Persistence Globals
pub static TAG_HWND: AtomicIsize = AtomicIsize::new(0);
pub static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
pub static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);

// CONTINUOUS MODE HOTKEY TRACKING
pub static mut TRIGGER_VK_CODE: u32 = 0;
pub static mut TRIGGER_MODIFIERS: u32 = 0;
pub static IS_HOTKEY_HELD: AtomicBool = AtomicBool::new(false);
pub static CONTINUOUS_ACTIVATED_THIS_SESSION: AtomicBool = AtomicBool::new(false);
pub static HOLD_DETECTED_THIS_SESSION: AtomicBool = AtomicBool::new(false);

// DEDUPLICATION: Timestamp of last instant process to debounce rapid calls
pub static LAST_INSTANT_PROCESS_TIME: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
// TOGGLE DETECTION: Timestamp of when badge was last shown (for toggle-off detection)
pub static LAST_BADGE_SHOW_TIME: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
// TOGGLE DETECTION: The preset index that last showed the badge
pub static LAST_BADGE_PRESET_IDX: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
// DRAG DETECTION: Mouse start position when selection begins
pub static MOUSE_START_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
pub static MOUSE_START_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
pub static PENDING_SHOW_ON_WARMUP: AtomicBool = AtomicBool::new(false);

// IMAGE CONTINUOUS MODE: Secondary badge visibility
pub static IMAGE_CONTINUOUS_BADGE_VISIBLE: AtomicBool = AtomicBool::new(false);
pub static IMAGE_CONTINUOUS_PENDING_SHOW: AtomicBool = AtomicBool::new(false);
pub static TEXT_BADGE_VISIBLE: AtomicBool = AtomicBool::new(false);

// Messages
pub const WM_APP_SHOW: u32 = WM_USER + 200;
pub const WM_APP_HIDE: u32 = WM_USER + 201;
pub const WM_APP_SHOW_IMAGE_BADGE: u32 = WM_USER + 202;
pub const WM_APP_HIDE_IMAGE_BADGE: u32 = WM_USER + 203;
pub const WM_APP_UPDATE_CONTINUOUS: u32 = WM_USER + 204;
pub const WM_APP_RESTORE_AFTER_CAPTURE: u32 = WM_USER + 205;

// Positioning constants
pub const OFFSET_X: i32 = -20;
pub const OFFSET_Y: i32 = -90;
pub const BADGE_WIDTH: i32 = 240;
pub const BADGE_HEIGHT: i32 = 140;

/// Reset internal selection state
pub fn reset_selection_internal_state() {
    let mut state = SELECTION_STATE.lock().unwrap();
    state.preset_idx = usize::MAX;
    state.is_selecting = false;
    state.is_processing = false;
    TEXT_BADGE_VISIBLE.store(false, Ordering::SeqCst);
    CONTINUOUS_ACTIVATED_THIS_SESSION.store(false, Ordering::SeqCst);
    HOLD_DETECTED_THIS_SESSION.store(false, Ordering::SeqCst);
    IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
    unsafe {
        TRIGGER_VK_CODE = 0;
        TRIGGER_MODIFIERS = 0;
    }
}

/// Reset UI state in WebView
pub fn reset_ui_state(initial_text: &str) {
    let state = SELECTION_STATE.lock().unwrap();
    if let Some(wv) = state.webview.as_ref() {
        let reset_js = format!("updateState(false, '{}')", initial_text);
        let _ = wv.evaluate_script(&reset_js);
    }
}

/// Processing guard that resets is_processing on drop
pub struct ProcessingGuard;

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        let mut state = SELECTION_STATE.lock().unwrap();
        state.is_processing = false;
    }
}
