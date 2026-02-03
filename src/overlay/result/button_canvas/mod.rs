//! Button canvas overlay for result windows
//!
//! This module provides a transparent overlay window that displays interactive
//! buttons for each registered markdown result window. The overlay uses a WebView
//! for rendering and handles mouse events to show/hide buttons based on proximity.

mod css;
mod html;
mod ipc;
mod js;
mod theme;
mod window;
mod wnd_proc;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, Ordering},
    Mutex,
};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{WebContext, WebView};


// Re-export window creation for external use
pub use window::create_canvas_window;

// Singleton canvas state
static CANVAS_HWND: AtomicIsize = AtomicIsize::new(0);
static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);
static IS_DRAGGING_EXTERNAL: AtomicBool = AtomicBool::new(false);
static REGISTER_CANVAS_CLASS: std::sync::Once = std::sync::Once::new();
static IS_INITIALIZING: AtomicBool = AtomicBool::new(false);

// Custom messages
const WM_APP_UPDATE_WINDOWS: u32 = WM_APP + 50;
const WM_APP_SHOW_CANVAS: u32 = WM_APP + 51;
const WM_APP_HIDE_CANVAS: u32 = WM_APP + 52;
const WM_APP_SEND_REFINE_TEXT: u32 = WM_APP + 53;

// Timer for cursor position polling
const CURSOR_POLL_TIMER_ID: usize = 1;

thread_local! {
    static CANVAS_WEBVIEW: RefCell<Option<WebView>> = RefCell::new(None);
    static CANVAS_WEB_CONTEXT: RefCell<Option<WebContext>> = RefCell::new(None);
}

lazy_static::lazy_static! {
    /// Tracks which result windows are in markdown mode and their positions
    /// Key: hwnd as isize, Value: (x, y, w, h)
    static ref MARKDOWN_WINDOWS: Mutex<HashMap<isize, (i32, i32, i32, i32)>> = Mutex::new(HashMap::new());
    static ref PENDING_REFINE_UPDATES: Mutex<HashMap<isize, String>> = Mutex::new(HashMap::new());
}

// Track last applied theme to avoid redundant injections
static LAST_THEME_IS_DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

// Global state for manual Rust-side dragging
static ACTIVE_DRAG_TARGET: AtomicIsize = AtomicIsize::new(0);
static DRAG_IS_GROUP: AtomicBool = AtomicBool::new(false);
static ACTIVE_DRAG_SNAPSHOT: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static LAST_DRAG_POS: Mutex<POINT> = Mutex::new(POINT { x: 0, y: 0 });
static START_DRAG_POS: Mutex<POINT> = Mutex::new(POINT { x: 0, y: 0 });

/// Get DPI scale factor (1.0 = 100%, 1.5 = 150%, 2.0 = 200%, etc.)
fn get_dpi_scale() -> f64 {
    let dpi = unsafe { GetDpiForSystem() };
    dpi as f64 / 96.0
}

/// Register a markdown window for button overlay
pub fn register_markdown_window(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    // Initialize on-demand if not warmed up
    if !IS_WARMED_UP.load(Ordering::SeqCst) && CANVAS_HWND.load(Ordering::SeqCst) == 0 {
        if !IS_INITIALIZING.swap(true, Ordering::SeqCst) {
            std::thread::spawn(|| {
                create_canvas_window();
            });
        }

        // Polling thread to auto-show once ready
        std::thread::spawn(move || {
            for _ in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if IS_WARMED_UP.load(Ordering::SeqCst) && CANVAS_HWND.load(Ordering::SeqCst) != 0 {
                    update_canvas();
                    show_canvas();
                    return;
                }
            }
        });
    }

    // Get window rect
    let rect = unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        r
    };

    {
        let mut windows = MARKDOWN_WINDOWS.lock().unwrap();
        windows.insert(
            hwnd_key,
            (
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            ),
        );
    }

    // Trigger canvas update
    update_canvas();
    show_canvas();
}

/// Unregister a markdown window
pub fn unregister_markdown_window(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    {
        let mut windows = MARKDOWN_WINDOWS.lock().unwrap();
        windows.remove(&hwnd_key);

        // If no more markdown windows, hide canvas
        if windows.is_empty() {
            hide_canvas();
        }
    }

    update_canvas();
}

/// Update window position (call when window moves/resizes)
pub fn update_window_position(hwnd: HWND) {
    update_window_position_internal(hwnd, true);
}

fn update_window_position_internal(hwnd: HWND, notify: bool) {
    let hwnd_key = hwnd.0 as isize;

    let rect = unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        r
    };

    {
        let mut windows = MARKDOWN_WINDOWS.lock().unwrap();
        if windows.contains_key(&hwnd_key) {
            windows.insert(
                hwnd_key,
                (
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                ),
            );
        }
    }

    if notify {
        update_canvas();
        update_canvas();
    }
}

/// Update window position directly in the register (skips GetWindowRect, faster for bulk)
pub fn update_window_position_direct(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    let hwnd_key = hwnd.0 as isize;
    let mut windows = MARKDOWN_WINDOWS.lock().unwrap();
    if windows.contains_key(&hwnd_key) {
        windows.insert(hwnd_key, (x, y, w, h));
    }
}

/// Send update to set the text in the refine input bar
pub fn send_refine_text_update(hwnd: HWND, text: &str, is_insert: bool) {
    let hwnd_key = hwnd.0 as isize;

    // Store in pending updates
    {
        let mut updates = PENDING_REFINE_UPDATES.lock().unwrap();
        updates.insert(hwnd_key, text.to_string());
    }

    // Notify canvas thread
    let canvas_hwnd = CANVAS_HWND.load(Ordering::SeqCst);
    if canvas_hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(canvas_hwnd as *mut _)),
                WM_APP_SEND_REFINE_TEXT,
                WPARAM(hwnd_key as usize),
                LPARAM(if is_insert { 1 } else { 0 }),
            );
        }
    }
}

/// Check if the button canvas is currently in drag mode
pub fn is_dragging() -> bool {
    IS_DRAGGING_EXTERNAL.load(Ordering::SeqCst)
}

/// Check if a point is within any registered result window bounds
pub fn is_point_over_result_window(x: i32, y: i32) -> bool {
    let canvas_hwnd = CANVAS_HWND.load(Ordering::SeqCst);
    if canvas_hwnd == 0 {
        return false;
    }

    let windows = MARKDOWN_WINDOWS.lock().unwrap();
    for (_hwnd, (wx, wy, ww, wh)) in windows.iter() {
        let padding = 60;
        if x >= wx - padding
            && x <= wx + ww + padding
            && y >= wy - padding
            && y <= wy + wh + padding
        {
            return true;
        }
    }
    false
}

/// Set drag mode (temporarily disable region clipping to prevent UI cutoff)
pub fn set_drag_mode(active: bool) {
    let canvas_hwnd = CANVAS_HWND.load(Ordering::SeqCst);
    if canvas_hwnd == 0 {
        return;
    }
    let hwnd = HWND(canvas_hwnd as *mut std::ffi::c_void);

    if active {
        IS_DRAGGING_EXTERNAL.store(true, Ordering::SeqCst);
        unsafe {
            let _ = SetWindowRgn(hwnd, None, true);
        }
    } else {
        IS_DRAGGING_EXTERNAL.store(false, Ordering::SeqCst);

        unsafe {
            let empty = CreateRectRgn(0, 0, 0, 0);
            let _ = SetWindowRgn(hwnd, Some(empty), true);
        }

        update_canvas();
    }
}

/// Update canvas with current window positions
pub fn update_canvas() {
    let canvas_hwnd = CANVAS_HWND.load(Ordering::SeqCst);
    if canvas_hwnd != 0 {
        let hwnd = HWND(canvas_hwnd as *mut std::ffi::c_void);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE_WINDOWS, WPARAM(0), LPARAM(0));
        }
    }
}

/// Show the canvas
fn show_canvas() {
    let canvas_hwnd = CANVAS_HWND.load(Ordering::SeqCst);
    if canvas_hwnd != 0 {
        let hwnd = HWND(canvas_hwnd as *mut std::ffi::c_void);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_SHOW_CANVAS, WPARAM(0), LPARAM(0));
        }
    }
}

/// Hide the canvas
fn hide_canvas() {
    let canvas_hwnd = CANVAS_HWND.load(Ordering::SeqCst);
    if canvas_hwnd != 0 {
        let hwnd = HWND(canvas_hwnd as *mut std::ffi::c_void);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE_CANVAS, WPARAM(0), LPARAM(0));
        }
    }
}
