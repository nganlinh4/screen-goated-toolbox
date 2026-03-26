// Tray Popup - Custom non-blocking popup window for tray icon menu
// Replaces native Windows tray context menu to avoid blocking the main UI thread

mod html;
mod native_menu;
mod render;
mod window;

use std::cell::RefCell;
use std::sync::{
    Once,
    atomic::{AtomicIsize, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebView};

static REGISTER_POPUP_CLASS: Once = Once::new();
static POPUP_HWND: AtomicIsize = AtomicIsize::new(0);
static IGNORE_FOCUS_LOSS_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// Warmup flag - tracks if the window has been created and is ready for instant display
static IS_WARMED_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IS_WARMING_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WARMUP_START_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
// Flag to track if WebView has permanently failed to initialize
static WEBVIEW_INIT_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// Custom window messages
const WM_APP_SHOW: u32 = WM_APP + 1;

thread_local! {
    static POPUP_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    // Shared WebContext for this thread using common data directory
    static POPUP_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
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
struct HwndWrapper(HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}
impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0.0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

/// Show the tray popup at cursor position
pub fn show_tray_popup() {
    unsafe {
        // Fallback to native menu if WebView failed completely
        if WEBVIEW_INIT_FAILED.load(Ordering::SeqCst) {
            native_menu::show_native_context_menu();
            return;
        }

        // Check if warmed up and window exists
        if !IS_WARMED_UP.load(Ordering::SeqCst) {
            // Not ready yet - trigger warmup and show notification
            warmup_tray_popup();

            let ui_lang = crate::APP.lock().unwrap().config.ui_language.clone();
            let locale = crate::gui::locale::LocaleText::get(&ui_lang);
            crate::overlay::auto_copy_badge::show_notification(locale.tray_popup_loading);

            // Spawn thread to wait for warmup completion and auto-show
            std::thread::spawn(move || {
                // Poll for 5 seconds (50 * 100ms)
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    // Check if ready
                    let ready = IS_WARMED_UP.load(Ordering::SeqCst)
                        && POPUP_HWND.load(Ordering::SeqCst) != 0;
                    if ready {
                        show_tray_popup();
                        return;
                    }
                }
            });
            return;
        }

        let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
        if hwnd_val == 0 {
            // Should be warmed up but handle missing? Retry warmup
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            warmup_tray_popup();
            return;
        }

        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

        // Check if window still valid logic...
        if !IsWindow(Some(hwnd)).as_bool() {
            // Window destroyed
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            POPUP_HWND.store(0, Ordering::SeqCst);
            warmup_tray_popup();
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

/// Hide the tray popup (preserves window for reuse)
pub fn hide_tray_popup() {
    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            // Just hide - don't destroy. Preserves WebView state for instant redisplay.
            let _ = KillTimer(Some(hwnd), 888);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Warmup the tray popup - creates hidden window with WebView for instant display later
pub fn warmup_tray_popup() {
    // Check if dead stuck (timestamp check)
    unsafe {
        let start_time = WARMUP_START_TIME.load(Ordering::SeqCst);
        let now = windows::Win32::System::SystemInformation::GetTickCount64();
        if start_time > 0 && (now - start_time) > 10000 {
            // Stuck for > 10s - force reset
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            POPUP_HWND.store(0, Ordering::SeqCst);
        }
    }

    // Only allow one warmup thread at a time
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    // Update timestamp
    unsafe {
        WARMUP_START_TIME.store(
            windows::Win32::System::SystemInformation::GetTickCount64(),
            Ordering::SeqCst,
        );
    }

    std::thread::spawn(|| {
        window::create_popup_window();
    });
}

/// Check if the tray popup is currently visible
/// Used by warmup logic to defer WebView2 initialization until popup closes
pub fn is_popup_open() -> bool {
    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe { IsWindowVisible(hwnd).as_bool() }
    } else {
        false
    }
}
