// Tray Popup - Custom non-blocking popup window for tray icon menu
// Replaces native Windows tray context menu to avoid blocking the main UI thread

mod html;
mod native_menu;
mod render;
mod window;

/// Material Symbols "check" glyph for the bubble-active checkmark. Centralized
/// so the server-rendered HTML, the client-side toggle JS, and the window.rs
/// update path all stay in sync (previously this string was triplicated).
pub(super) const BUBBLE_CHECK_SVG: &str = r#"<svg class="check-icon" viewBox="0 0 24 24" fill="currentColor"><path d="m9.55 18l-5.7-5.7l1.425-1.425L9.55 15.15l9.175-9.175L20.15 7.4z"/></svg>"#;

use std::cell::RefCell;
use std::sync::{
    Once,
    atomic::{AtomicIsize, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebView};

static REGISTER_POPUP_CLASS: Once = Once::new();
static POPUP_HWND: AtomicIsize = AtomicIsize::new(0);
static IGNORE_FOCUS_LOSS_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PENDING_SHOW_ANCHOR_X: AtomicIsize = AtomicIsize::new(0);
static PENDING_SHOW_ANCHOR_Y: AtomicIsize = AtomicIsize::new(0);
static HAS_PENDING_SHOW_ANCHOR: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// The tray WebView is initialized on first use, then retained while the app is running.
static IS_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IS_INITIALIZING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static INIT_START_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SHOW_WHEN_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
// Flag to track if WebView has permanently failed to initialize
static WEBVIEW_INIT_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// Custom window messages
const WM_APP_SHOW: u32 = WM_APP + 1;
const WM_APP_UPDATE_THEME: u32 = WM_APP + 2;

thread_local! {
    static POPUP_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    // Shared WebContext for this thread using common data directory
    static POPUP_WEB_CONTEXT: RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> = const { RefCell::new(None) };
}

const BASE_POPUP_WIDTH: i32 = 240;
const BASE_POPUP_HEIGHT: i32 = 186; // Base height at 100% scaling (96 DPI) - includes restore row
const POPUP_SURFACE_INSET: i32 = 6;
const RESTORE_FLYOUT_WIDTH: i32 = 236;
const RESTORE_FLYOUT_GAP: i32 = 10;
const RESTORE_FLYOUT_OPTION_HEIGHT: i32 = 28;
const RESTORE_FLYOUT_VERTICAL_PADDING: i32 = 8;
const RESTORE_FLYOUT_TOP_INSET: i32 = 6;
const RESTORE_FLYOUT_PREFERRED_TOP: i32 = 100;

/// Get DPI-scaled dimension
fn get_scaled_dimension(base: i32) -> i32 {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
    // Scale: 96 DPI = 100%, 120 DPI = 125%, 144 DPI = 150%, etc.
    // Using 93 instead of 96 provides a small buffer (~3%) to ensure content fits comfortably
    (base * dpi as i32) / 93
}

fn popup_window_dimensions() -> (i32, i32) {
    let inset = get_scaled_dimension(POPUP_SURFACE_INSET);
    let width = get_scaled_dimension(BASE_POPUP_WIDTH)
        + inset * 2
        + if crate::overlay::result::recent_restore_option_counts().is_empty() {
            0
        } else {
            get_scaled_dimension(RESTORE_FLYOUT_GAP + RESTORE_FLYOUT_WIDTH)
        };
    let height = get_scaled_dimension(BASE_POPUP_HEIGHT) + inset * 2;
    (width, height)
}

unsafe fn set_popup_bounds(hwnd: HWND, x: i32, y: i32) {
    let (popup_width, popup_height) = popup_window_dimensions();
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let popup_x = x.max(0).min((screen_w - popup_width).max(0));
    let popup_y = y.max(0).min((screen_h - popup_height).max(0));

    POPUP_WEBVIEW.with(|cell| {
        if let Some(webview) = cell.borrow().as_ref() {
            let _ = webview.set_bounds(Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    popup_width as u32,
                    popup_height as u32,
                )),
            });
        }
    });

    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            popup_x,
            popup_y,
            popup_width,
            popup_height,
            SWP_NOZORDER,
        );
    }
}

// HWND wrapper for wry
use crate::win_types::HwndWrapper;

/// Show the tray popup at cursor position
pub fn show_tray_popup() {
    unsafe {
        if !crate::runtime_support::webview2_runtime_installed() {
            native_menu::show_native_context_menu();
            return;
        }

        // Fallback to native menu if WebView failed completely
        if WEBVIEW_INIT_FAILED.load(Ordering::SeqCst) {
            native_menu::show_native_context_menu();
            return;
        }

        let is_initializing = IS_INITIALIZING.load(Ordering::SeqCst);
        let has_pending_anchor = HAS_PENDING_SHOW_ANCHOR.load(Ordering::SeqCst);

        if !(is_initializing && has_pending_anchor) {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() {
                PENDING_SHOW_ANCHOR_X.store(pt.x as isize, Ordering::SeqCst);
                PENDING_SHOW_ANCHOR_Y.store(pt.y as isize, Ordering::SeqCst);
                HAS_PENDING_SHOW_ANCHOR.store(true, Ordering::SeqCst);
            }
        }

        if !IS_READY.load(Ordering::SeqCst) {
            SHOW_WHEN_READY.store(true, Ordering::SeqCst);
            ensure_tray_popup_initialized();
            return;
        }

        let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
        if hwnd_val == 0 {
            IS_READY.store(false, Ordering::SeqCst);
            SHOW_WHEN_READY.store(true, Ordering::SeqCst);
            ensure_tray_popup_initialized();
            return;
        }

        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

        // Check if window still valid logic...
        if !IsWindow(Some(hwnd)).as_bool() {
            IS_READY.store(false, Ordering::SeqCst);
            POPUP_HWND.store(0, Ordering::SeqCst);
            SHOW_WHEN_READY.store(true, Ordering::SeqCst);
            ensure_tray_popup_initialized();
            return;
        }

        // Check if already visible
        if IsWindowVisible(hwnd).as_bool() {
            hide_tray_popup();
            return;
        }

        // Post message to show
        let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
    }
}

/// Hide the tray popup while retaining its on-demand-initialized window.
pub fn hide_tray_popup() {
    SHOW_WHEN_READY.store(false, Ordering::SeqCst);
    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            // Keep the initialized WebView for later tray clicks.
            let _ = KillTimer(Some(hwnd), 888);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub fn update_theme(is_dark: bool) {
    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_UPDATE_THEME,
                WPARAM(usize::from(is_dark)),
                LPARAM(0),
            );
        }
    }
}

fn ensure_tray_popup_initialized() {
    // A hung WebView2 build cannot be safely duplicated. Keep the original attempt
    // serialized; subsequent clicks can use the native fallback after the timeout.
    unsafe {
        let start_time = INIT_START_TIME.load(Ordering::SeqCst);
        let now = windows::Win32::System::SystemInformation::GetTickCount64();
        if start_time > 0 && (now - start_time) > 10000 {
            SHOW_WHEN_READY.store(false, Ordering::SeqCst);
            native_menu::show_native_context_menu();
            return;
        }
    }

    if IS_INITIALIZING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    unsafe {
        INIT_START_TIME.store(
            windows::Win32::System::SystemInformation::GetTickCount64(),
            Ordering::SeqCst,
        );
    }

    std::thread::spawn(|| {
        window::create_popup_window();
    });
}
