// --- TEXT SELECTION STATE ---
// Shared state, atomics, and constants for text selection badge.

use std::sync::{
    Arc, LazyLock, Mutex, Once,
    atomic::{AtomicBool, AtomicIsize, Ordering},
};
use windows::Win32::UI::WindowsAndMessaging::*;

// --- SHARED STATE ---
pub struct TextSelectionState {
    pub preset_idx: Option<usize>,
    pub generation: u64,
    pub is_selecting: bool,
    pub is_processing: bool,
    pub hook_handle: HHOOK,
}
unsafe impl Send for TextSelectionState {}

pub static SELECTION_STATE: Mutex<TextSelectionState> = Mutex::new(TextSelectionState {
    preset_idx: None,
    generation: 0,
    is_selecting: false,
    is_processing: false,
    hook_handle: HHOOK(std::ptr::null_mut()),
});

#[derive(Default)]
struct SelectionLifecycleState {
    active_generation: u64,
    pending_show_generation: Option<u64>,
}

pub struct SelectionLifecycle {
    state: Mutex<SelectionLifecycleState>,
}

impl SelectionLifecycle {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(SelectionLifecycleState {
                active_generation: 0,
                pending_show_generation: None,
            }),
        }
    }

    pub fn begin(&self) -> u64 {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.active_generation = next_generation(state.active_generation);
        state.pending_show_generation = None;
        state.active_generation
    }

    pub fn cancel(&self) -> u64 {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.active_generation = next_generation(state.active_generation);
        state.pending_show_generation = None;
        state.active_generation
    }

    pub fn queue_show(&self, generation: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.active_generation != generation {
            return false;
        }
        state.pending_show_generation = Some(generation);
        true
    }

    pub fn take_pending_show(&self) -> Option<u64> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .pending_show_generation
            .take()
            .filter(|generation| *generation == state.active_generation)
    }

    pub fn is_current(&self, generation: u64) -> bool {
        generation != 0
            && self
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .active_generation
                == generation
    }
}

fn next_generation(current: u64) -> u64 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

pub static SELECTION_LIFECYCLE: SelectionLifecycle = SelectionLifecycle::new();
pub static SELECTION_TRANSITION_LOCK: Mutex<()> = Mutex::new(());

pub static REGISTER_TAG_CLASS: Once = Once::new();

pub static TAG_ABORT_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static INITIAL_TEXT_GLOBAL: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::from("Select text...")));

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
    state.preset_idx = None;
    state.generation = 0;
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
    crate::overlay::status_compositor::selection_update(false, initial_text.to_string());
}

/// Processing guard that resets is_processing on drop
pub struct ProcessingGuard;

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        let mut state = SELECTION_STATE.lock().unwrap();
        state.is_processing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::SelectionLifecycle;

    #[test]
    fn cancelled_warmup_show_is_discarded() {
        let lifecycle = SelectionLifecycle::new();
        let generation = lifecycle.begin();
        assert!(lifecycle.queue_show(generation));

        let cancelled_generation = lifecycle.cancel();

        assert_eq!(lifecycle.take_pending_show(), None);
        assert!(!lifecycle.is_current(generation));
        assert!(lifecycle.is_current(cancelled_generation));
    }

    #[test]
    fn stale_show_cannot_replace_a_newer_selection() {
        let lifecycle = SelectionLifecycle::new();
        let stale_generation = lifecycle.begin();
        lifecycle.cancel();
        let current_generation = lifecycle.begin();

        assert!(!lifecycle.queue_show(stale_generation));
        assert!(lifecycle.queue_show(current_generation));
        assert_eq!(lifecycle.take_pending_show(), Some(current_generation));
    }
}
