//! Image Continuous Mode
//!
//! A non-blocking image selection mode that allows users to work normally while
//! being able to capture screen regions using right-click/drag gestures.

mod graphics;
mod hooks;
mod logic;

pub(crate) use graphics::{dim_pixels, render_frozen_with_selection, FrozenSelectionRender};

use crate::win_types::SendHbitmap;
use crate::GdiCapture;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// === STATE ===

static IS_ACTIVE: AtomicBool = AtomicBool::new(false);
static PRESET_IDX: AtomicUsize = AtomicUsize::new(0);
static TRIGGER_VK: AtomicU32 = AtomicU32::new(0);
static TRIGGER_ID: AtomicI32 = AtomicI32::new(0);
static HAS_RELEASED_SINCE_ACTIVATION: AtomicBool = AtomicBool::new(false);

// Gesture state
static RIGHT_DOWN: AtomicBool = AtomicBool::new(false);
static START_X: AtomicI32 = AtomicI32::new(0);
static START_Y: AtomicI32 = AtomicI32::new(0);
static LAST_X: AtomicI32 = AtomicI32::new(0);
static LAST_Y: AtomicI32 = AtomicI32::new(0);

// Animation State for Dim Screen
static mut CURRENT_DIM_ALPHA: u8 = 0;
const TARGET_DIM_ALPHA: u8 = 100;
const DIM_FADE_STEP: u8 = 20; // Fast fade (approx 5 frames @ 60fps)
const DIM_TIMER_ID: usize = 5;

// Zoom state (while right is held + wheel)
static ZOOM_LEVEL: Mutex<f32> = Mutex::new(1.0);

// Per-gesture capture
static GESTURE_CAPTURE: Mutex<Option<GdiCapture>> = Mutex::new(None);

// Magnification API
type MagInitializeFn = unsafe extern "system" fn() -> windows_core::BOOL;
type MagUninitializeFn = unsafe extern "system" fn() -> windows_core::BOOL;
type MagSetFullscreenTransformFn = unsafe extern "system" fn(f32, i32, i32) -> windows_core::BOOL;

static mut MAG_DLL_LOADED: bool = false;
static mut MAG_INITIALIZE_FN: Option<MagInitializeFn> = None;
static mut MAG_UNINITIALIZE_FN: Option<MagUninitializeFn> = None;
static mut MAG_SET_FULLSCREEN_TRANSFORM_FN: Option<MagSetFullscreenTransformFn> = None;
static MAG_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Cached overlay DIB section (avoid per-frame allocation — reused across renders)
static mut OVERLAY_DIB: SendHbitmap = SendHbitmap(HBITMAP(std::ptr::null_mut()));
static mut OVERLAY_DIB_BITS: *mut u32 = std::ptr::null_mut();
static mut OVERLAY_DIB_W: i32 = 0;
static mut OVERLAY_DIB_H: i32 = 0;

// Window and Hook Handles (Managed by the thread)
static OVERLAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static RECT_OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);

// Hotkey tracking for exit
static HOTKEY_NAME: Mutex<String> = Mutex::new(String::new());

// === PUBLIC API ===

pub fn is_active() -> bool {
    IS_ACTIVE.load(Ordering::SeqCst)
}

pub fn enter(preset_idx: usize, hotkey_name: String, hotkey_id: i32) {
    if IS_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    PRESET_IDX.store(preset_idx, Ordering::SeqCst);
    TRIGGER_ID.store(hotkey_id, Ordering::SeqCst);
    *HOTKEY_NAME.lock().unwrap() = hotkey_name;

    // Reset state
    RIGHT_DOWN.store(false, Ordering::SeqCst);
    *ZOOM_LEVEL.lock().unwrap() = 1.0;
    *GESTURE_CAPTURE.lock().unwrap() = None;
    unsafe {
        CURRENT_DIM_ALPHA = 0;
    }

    IS_ACTIVE.store(true, Ordering::SeqCst);

    // Track trigger hotkey for safety (prevent flicker when holding)
    if let Some((_, vk)) = crate::overlay::continuous_mode::get_current_hotkey_info() {
        TRIGGER_VK.store(vk, Ordering::SeqCst);
    } else {
        TRIGGER_VK.store(0, Ordering::SeqCst);
    }
    HAS_RELEASED_SINCE_ACTIVATION.store(false, Ordering::SeqCst);

    // Show the badge (handled by text_selection module, reuses Webview)
    crate::overlay::text_selection::set_image_continuous_badge(true);

    // NEW: Show activation notification manually since we detached from global state
    let (p_id, h_name) = {
        let name = HOTKEY_NAME.lock().unwrap().clone();
        if let Ok(app) = crate::APP.lock() {
            let id = app
                .config
                .presets
                .get(preset_idx)
                .map(|p| p.id.clone())
                .unwrap_or_default();
            (id, name)
        } else {
            (String::new(), name)
        }
    };
    if !p_id.is_empty() {
        crate::overlay::continuous_mode::show_image_continuous_notification(&p_id, &h_name);
    }

    // Spawn the dedicated thread that owns the Window AND the Hooks
    std::thread::spawn(|| {
        overlay_thread_entry();
    });

    crate::log_info!("[ImageContinuous] Mode entered for preset {}", preset_idx);
}

pub fn exit() {
    if !IS_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    // If currently dragging, cancel the drag instead of exiting the mode.
    if RIGHT_DOWN.swap(false, Ordering::SeqCst) {
        *GESTURE_CAPTURE.lock().unwrap() = None;
        crate::log_info!("[ImageContinuous] Drag cancelled via exit()");
        return;
    }

    crate::log_info!("[ImageContinuous] Mode exit() called");

    IS_ACTIVE.store(false, Ordering::SeqCst);

    // Hide badge
    crate::overlay::text_selection::set_image_continuous_badge(false);

    // Signal the thread to exit by posting WM_QUIT
    let thread_id = OVERLAY_THREAD_ID.load(Ordering::SeqCst);
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }

    crate::log_info!("[ImageContinuous] Mode exited (Quit signal posted)");
}

pub fn get_preset_idx() -> usize {
    PRESET_IDX.load(Ordering::SeqCst)
}

pub fn get_trigger_id() -> i32 {
    TRIGGER_ID.load(Ordering::SeqCst)
}

pub fn can_exit_now() -> bool {
    // We can exit if the hotkey was released at least once since we entered the mode.
    // This prevents the initial "hold" that activated the mode from immediately toggling it off.
    HAS_RELEASED_SINCE_ACTIVATION.load(Ordering::SeqCst)
}

// === THREAD ENTRY POINT ===

fn overlay_thread_entry() {
    unsafe {
        let thread_id = GetCurrentThreadId();
        OVERLAY_THREAD_ID.store(thread_id, Ordering::SeqCst);

        // 1. Create Overlay Window
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = windows::core::w!("SGT_ImageContinuousOverlay");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(graphics::rect_overlay_wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap(),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        // WS_EX_TRANSPARENT makes it click-through (important! we intercept via hooks, not window)
        // WS_EX_TOOLWINDOW hides from alt-tab
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            class_name,
            windows::core::w!(""),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        );

        if let Ok(h) = hwnd {
            RECT_OVERLAY_HWND.store(h.0 as isize, Ordering::SeqCst);
            // Initialize transparent
            graphics::sync_rect_overlay(h);
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        } else {
            return;
        }

        // 2. Install Hooks on THIS thread
        // Because we have a message loop below, these hooks will stay alive.
        let mouse_hook =
            SetWindowsHookExW(WH_MOUSE_LL, Some(hooks::mouse_hook_proc), Some(instance.into()), 0);

        let kb_hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hooks::keyboard_hook_proc),
            Some(instance.into()),
            0,
        );

        // 3. Message Loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_QUIT {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // 4. Cleanup
        if let Ok(h) = mouse_hook {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Ok(h) = kb_hook {
            let _ = UnhookWindowsHookEx(h);
        }

        if let Ok(h) = hwnd {
            let _ = DestroyWindow(h);
        }

        // Reset Magnification
        logic::reset_magnification();

        // Clear capture and cached DIB
        *GESTURE_CAPTURE.lock().unwrap() = None;
        if !std::ptr::addr_of!(OVERLAY_DIB).read().0.is_invalid() {
            let _ = DeleteObject(OVERLAY_DIB.0.into());
            OVERLAY_DIB = SendHbitmap(HBITMAP(std::ptr::null_mut()));
            OVERLAY_DIB_BITS = std::ptr::null_mut();
            OVERLAY_DIB_W = 0;
            OVERLAY_DIB_H = 0;
        }
        RECT_OVERLAY_HWND.store(0, Ordering::SeqCst);
        OVERLAY_THREAD_ID.store(0, Ordering::SeqCst);
    }
}
