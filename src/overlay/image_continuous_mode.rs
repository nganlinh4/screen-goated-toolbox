//! Image Continuous Mode
//!
//! A non-blocking image selection mode that allows users to work normally while
//! being able to capture screen regions using right-click/drag gestures.

use crate::overlay::process::start_processing_pipeline;
use crate::overlay::selection::extract_crop_from_hbitmap_public;
use crate::win_types::SendHbitmap;
use crate::{GdiCapture, APP};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
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
const ZOOM_STEP: f32 = 0.25;
const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 4.0;

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
            lpfnWndProc: Some(rect_overlay_wnd_proc),
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
            sync_rect_overlay(h);
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        } else {
            return;
        }

        // 2. Install Hooks on THIS thread
        // Because we have a message loop below, these hooks will stay alive.
        let mouse_hook =
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), Some(instance.into()), 0);

        let kb_hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook_proc),
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
        reset_magnification();

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

// === HOOK PROCS ===

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT { unsafe {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let vk = kbd.vkCode;
    let flags = kbd.flags;

    // Check if this is an injected event (from SendInput)
    let is_injected = (flags.0 & 0x10) != 0; // LLKHF_INJECTED = 0x10

    if wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize {
        if vk == VK_ESCAPE.0 as u32 {
            // Only handle REAL user-pressed ESC, not injected events
            if is_injected {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            // exit() handles both cases: cancel drag if dragging, or full exit.
            // Other hooks (text_selection, etc.) may also call exit() on the same
            // ESC keystroke — the DRAG_JUST_CANCELLED flag absorbs duplicates.
            exit();
            return LRESULT(1);
        }
    } else if (wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize)
        && vk == TRIGGER_VK.load(Ordering::SeqCst)
    {
        HAS_RELEASED_SINCE_ACTIVATION.store(true, Ordering::SeqCst);
    }
    CallNextHookEx(None, code, wparam, lparam)
}}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT { unsafe {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let mouse_struct = &*(lparam.0 as *const MSLLHOOKSTRUCT);
    let pt = mouse_struct.pt;

    match wparam.0 as u32 {
        WM_RBUTTONDOWN => {
            // Check if clicking on our own UI (Badge, etc)
            let hwnd_under_mouse = WindowFromPoint(pt);
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd_under_mouse, Some(&mut pid));
            let our_pid = std::process::id();

            // Allow clicking on our own badge/UI without triggering capture
            if pid == our_pid {
                return CallNextHookEx(None, code, wparam, lparam);
            }

            // Start Gesture
            RIGHT_DOWN.store(true, Ordering::SeqCst);
            START_X.store(pt.x, Ordering::SeqCst);
            START_Y.store(pt.y, Ordering::SeqCst);
            LAST_X.store(pt.x, Ordering::SeqCst);
            LAST_Y.store(pt.y, Ordering::SeqCst);

            // CRITICAL: Hide all badges BEFORE capture to prevent them appearing in screenshot
            // ShowWindow(SW_HIDE) is synchronous — window is hidden immediately in the
            // window manager before BitBlt, no sleep needed.
            crate::overlay::text_selection::hide_all_badges_for_capture();

            // Capture screen NOW at start of drag
            // This ensures we get what the user sees before drawing box
            if let Ok(capture) = capture_screen_now() {
                *GESTURE_CAPTURE.lock().unwrap() = Some(capture);
            }

            // Restore badges after capture is complete
            crate::overlay::text_selection::restore_badges_after_capture();

            // Trigger fade-in animation
            let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                let hwnd = HWND(hwnd_val as *mut _);
                SetTimer(Some(hwnd), DIM_TIMER_ID, 16, None);

                // Remove WS_EX_TRANSPARENT so overlay captures mouse → crosshair cursor
                let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(hwnd, GWL_EXSTYLE, style & !(WS_EX_TRANSPARENT.0 as i32));
                SetCursor(Some(LoadCursorW(None, IDC_CROSS).unwrap()));
            }

            return LRESULT(1); // Swallow event
        }

        WM_MOUSEMOVE => {
            if RIGHT_DOWN.load(Ordering::SeqCst) {
                LAST_X.store(pt.x, Ordering::SeqCst);
                LAST_Y.store(pt.y, Ordering::SeqCst);
                // Timer (60fps) picks up latest position — no blocking in hook.
            }
        }

        WM_RBUTTONUP => {
            if RIGHT_DOWN.load(Ordering::SeqCst) {
                RIGHT_DOWN.store(false, Ordering::SeqCst);

                // Trigger fade-out animation
                let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
                if hwnd_val != 0 {
                    SetTimer(Some(HWND(hwnd_val as *mut _)), DIM_TIMER_ID, 16, None);
                }

                reset_magnification();

                let start_x = START_X.load(Ordering::SeqCst);
                let start_y = START_Y.load(Ordering::SeqCst);
                let dx = (pt.x - start_x).abs();
                let dy = (pt.y - start_y).abs();

                if dx <= 5 && dy <= 5 {
                    handle_color_pick(pt);
                } else {
                    handle_region_capture(start_x, start_y, pt.x, pt.y);
                }

                // Restore WS_EX_TRANSPARENT for click-through behavior
                let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
                if hwnd_val != 0 {
                    let hwnd = HWND(hwnd_val as *mut _);
                    let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT.0 as i32);
                }

                // Clean up capture (fade-out uses transparent overlay)
                *GESTURE_CAPTURE.lock().unwrap() = None;

                return LRESULT(1);
            }
        }

        WM_MOUSEWHEEL => {
            if RIGHT_DOWN.load(Ordering::SeqCst) {
                let delta = ((mouse_struct.mouseData >> 16) as i16) as i32;
                handle_zoom(delta, pt);
                return LRESULT(1);
            }
        }

        _ => {}
    }

    CallNextHookEx(None, code, wparam, lparam)
}}

// === LOGIC ===

unsafe fn handle_color_pick(pt: POINT) { unsafe {
    let capture_guard = GESTURE_CAPTURE.lock().unwrap();
    if let Some(capture) = capture_guard.as_ref() {
        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let old_bmp = SelectObject(hdc_mem, capture.hbitmap.into());

        let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);

        let color = GetPixel(hdc_mem, pt.x - sx, pt.y - sy);

        SelectObject(hdc_mem, old_bmp);
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);

        let r = (color.0 & 0xFF) as u8;
        let g = ((color.0 >> 8) & 0xFF) as u8;
        let b = ((color.0 >> 16) & 0xFF) as u8;
        let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);

        crate::overlay::utils::copy_to_clipboard(&hex, HWND::default());
        crate::overlay::auto_copy_badge::show_auto_copy_badge_text(&hex);
    }
}}

fn handle_region_capture(start_x: i32, start_y: i32, end_x: i32, end_y: i32) {
    let rect = RECT {
        left: start_x.min(end_x),
        top: start_y.min(end_y),
        right: start_x.max(end_x),
        bottom: start_y.max(end_y),
    };

    if (rect.right - rect.left) < 5 || (rect.bottom - rect.top) < 5 {
        return;
    }

    let preset_idx = PRESET_IDX.load(Ordering::SeqCst);

    let capture_guard = GESTURE_CAPTURE.lock().unwrap();
    if let Some(capture) = capture_guard.as_ref() {
        let cropped = extract_crop_from_hbitmap_public(capture, rect);

        // Prepare config on main app
        let (config, preset) = if let Ok(mut app) = APP.lock() {
            app.config.active_preset_idx = preset_idx;
            (app.config.clone(), app.config.presets[preset_idx].clone())
        } else {
            return;
        };

        // Processing on separate thread
        std::thread::spawn(move || {
            start_processing_pipeline(cropped, rect, config, preset);
        });
    }
}

fn handle_zoom(delta: i32, cursor: POINT) {
    let mut zoom = ZOOM_LEVEL.lock().unwrap();
    if delta > 0 {
        *zoom = (*zoom + ZOOM_STEP).min(MAX_ZOOM);
    } else {
        *zoom = (*zoom - ZOOM_STEP).max(MIN_ZOOM);
    }

    let z = *zoom;
    drop(zoom);

    if z > 1.01 {
        ensure_magnification_initialized();
        unsafe {
            if let Some(func) = MAG_SET_FULLSCREEN_TRANSFORM_FN {
                let sw = GetSystemMetrics(SM_CXVIRTUALSCREEN) as f32;
                let sh = GetSystemMetrics(SM_CYVIRTUALSCREEN) as f32;
                let sx = GetSystemMetrics(SM_XVIRTUALSCREEN) as f32;
                let sy = GetSystemMetrics(SM_YVIRTUALSCREEN) as f32;

                let view_w = sw / z;
                let view_h = sh / z;

                let mut ox = cursor.x as f32 - view_w / 2.0;
                let mut oy = cursor.y as f32 - view_h / 2.0;

                ox = ox.max(sx).min(sx + sw - view_w);
                oy = oy.max(sy).min(sy + sh - view_h);

                let _ = func(z, ox as i32, oy as i32);
            }
        }
    } else {
        reset_magnification();
    }
}

fn reset_magnification() {
    *ZOOM_LEVEL.lock().unwrap() = 1.0;
    if MAG_INITIALIZED.load(Ordering::SeqCst) {
        unsafe {
            if let Some(func) = MAG_SET_FULLSCREEN_TRANSFORM_FN {
                let _ = func(1.0, 0, 0);
            }
        }
    }
}

#[allow(static_mut_refs)]
fn ensure_magnification_initialized() {
    if MAG_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }
    unsafe {
        if !MAG_DLL_LOADED
            && let Ok(lib) = LoadLibraryW(windows::core::w!("Magnification.dll")) {
                if let Some(init) = GetProcAddress(lib, windows::core::s!("MagInitialize")) {
                    MAG_INITIALIZE_FN = Some(std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        MagInitializeFn,
                    >(init));
                    if let Some(f) = MAG_INITIALIZE_FN {
                        let _ = f();
                    }
                }
                if let Some(u) = GetProcAddress(lib, windows::core::s!("MagUninitialize")) {
                    MAG_UNINITIALIZE_FN = Some(std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        MagUninitializeFn,
                    >(u));
                }
                if let Some(s) = GetProcAddress(lib, windows::core::s!("MagSetFullscreenTransform"))
                {
                    MAG_SET_FULLSCREEN_TRANSFORM_FN = Some(std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        MagSetFullscreenTransformFn,
                    >(s));
                }
                MAG_DLL_LOADED = true;
                MAG_INITIALIZED.store(true, Ordering::SeqCst);
            }
    }
}

// === GRAPHICS ===

unsafe fn capture_screen_now() -> anyhow::Result<GdiCapture> { unsafe {
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

    let hdc_screen = GetDC(None);
    let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
    let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
    let _ = SelectObject(hdc_mem, hbm.into());

    BitBlt(hdc_mem, 0, 0, w, h, Some(hdc_screen), x, y, SRCCOPY)?;

    let _ = DeleteDC(hdc_mem);
    let _ = ReleaseDC(None, hdc_screen);

    Ok(GdiCapture {
        hbitmap: hbm,
        width: w,
        height: h,
    })
}}

/// Darken pixels by dim factor and set alpha to 0xFF.
/// inv_dim_256 = 256 - dim_alpha (256 = no dim, 0 = full black).
#[inline]
pub(crate) fn dim_pixels(pixels: &mut [u32], inv_dim_256: u32) {
    for px in pixels.iter_mut() {
        let v = *px;
        let r = (((v >> 16) & 0xFF) * inv_dim_256) >> 8;
        let g = (((v >> 8) & 0xFF) * inv_dim_256) >> 8;
        let b = ((v & 0xFF) * inv_dim_256) >> 8;
        *px = 0xFF000000 | (r << 16) | (g << 8) | b;
    }
}

/// Fix alpha to 0xFF (HBITMAP from BitBlt has alpha=0).
#[inline]
pub(crate) fn fix_alpha(pixels: &mut [u32]) {
    for px in pixels.iter_mut() {
        *px |= 0xFF000000;
    }
}

/// Render frozen frame with dim outside selection + SDF border.
/// All pixels end up alpha=0xFF (fully opaque overlay).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_frozen_with_selection(
    pixels: &mut [u32],
    w: i32,
    h: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    dim_alpha: u8,
) {
    let clear_l = left.max(0).min(w) as usize;
    let clear_r = right.max(0).min(w) as usize;
    let inv_dim = 256u32 - dim_alpha as u32;
    let dim_f = dim_alpha as f32 / 255.0;

    // SDF parameters
    let default_radius = 8.0f32;
    let border_width = 2.0f32;
    let hw = (right - left) as f32 / 2.0;
    let hh_f = (bottom - top) as f32 / 2.0;
    let cx = left as f32 + hw;
    let cy = top as f32 + hh_f;
    let radius = default_radius.min(hw).min(hh_f);

    let b_left = (left - 10).max(0);
    let b_right = (right + 10).min(w);
    let b_top_y = (top - 10).max(0);
    let b_bottom_y = (bottom + 10).min(h);
    let rad_int = radius.ceil() as i32;
    let top_band_end = (top + rad_int).min(b_bottom_y);
    let bottom_band_start = (bottom - rad_int).max(top_band_end);
    let len = pixels.len();

    for y in 0..h {
        let row_start = (y * w) as usize;
        let row_end = (row_start + w as usize).min(len);
        let row = &mut pixels[row_start..row_end];

        if y < b_top_y || y >= b_bottom_y {
            // Far from selection: dim entire row
            dim_pixels(row, inv_dim);
        } else if y >= top_band_end && y < bottom_band_start {
            // Middle band: dim left/right, leave selection undimmed, draw border
            dim_pixels(&mut row[..clear_l], inv_dim);
            fix_alpha(&mut row[clear_l..clear_r]);
            for bx in 0..2usize {
                if clear_l + bx < row.len() {
                    row[clear_l + bx] = 0xFFFFFFFF;
                }
                if clear_r > bx && clear_r - 1 - bx < row.len() {
                    row[clear_r - 1 - bx] = 0xFFFFFFFF;
                }
            }
            dim_pixels(&mut row[clear_r..], inv_dim);
        } else {
            // Corner band: dim outside SDF zone, SDF per-pixel inside zone
            let b_l = b_left.max(0) as usize;
            let b_r = (b_right as usize).min(row.len());
            dim_pixels(&mut row[..b_l], inv_dim);
            let py = y as f32 + 0.5;
            for px_int in b_left..b_right {
                let xi = px_int as usize;
                if xi >= row.len() {
                    continue;
                }
                let px_f = px_int as f32 + 0.5;
                let dx = (px_f - cx).abs() - (hw - radius);
                let dy = (py - cy).abs() - (hh_f - radius);
                let dist = if dx > 0.0 && dy > 0.0 {
                    (dx * dx + dy * dy).sqrt() - radius
                } else {
                    dx.max(dy) - radius
                };

                let alpha_outer = (0.5 - dist).clamp(0.0, 1.0);
                let alpha_inner = (0.5 - (dist + border_width)).clamp(0.0, 1.0);
                let border_alpha = alpha_outer - alpha_inner;
                let pixel_dim = dim_f * (1.0 - alpha_outer);
                let inv_pd = 1.0 - pixel_dim;

                let v = row[xi];
                let r = ((v >> 16) & 0xFF) as f32;
                let g = ((v >> 8) & 0xFF) as f32;
                let b = (v & 0xFF) as f32;
                let dr = r * inv_pd;
                let dg = g * inv_pd;
                let db = b * inv_pd;

                let inv_ba = 1.0 - border_alpha;
                let fr = (dr * inv_ba + 255.0 * border_alpha) as u32;
                let fg = (dg * inv_ba + 255.0 * border_alpha) as u32;
                let fb = (db * inv_ba + 255.0 * border_alpha) as u32;
                row[xi] = 0xFF000000 | (fr.min(255) << 16) | (fg.min(255) << 8) | fb.min(255);
            }
            dim_pixels(&mut row[b_r..], inv_dim);
        }
    }
}

#[allow(static_mut_refs)]
unsafe fn sync_rect_overlay(hwnd: HWND) { unsafe {
    let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);

    if w <= 0 || h <= 0 {
        return;
    }

    // 1. Cache DIB section (avoid per-frame allocation of ~8MB)
    if OVERLAY_DIB_W != w || OVERLAY_DIB_H != h {
        if !std::ptr::addr_of!(OVERLAY_DIB).read().0.is_invalid() {
            let _ = DeleteObject(OVERLAY_DIB.0.into());
            OVERLAY_DIB_BITS = std::ptr::null_mut();
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let hdc_screen = GetDC(None);
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(Some(hdc_screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(None, hdc_screen);

        if let Ok(dib) = hbm {
            OVERLAY_DIB = SendHbitmap(dib);
            OVERLAY_DIB_BITS = bits as *mut u32;
            OVERLAY_DIB_W = w;
            OVERLAY_DIB_H = h;
        } else {
            return;
        }
    }

    // 2. Render into the cached DIB
    let hdc_screen = GetDC(None);
    let mem_dc = CreateCompatibleDC(Some(hdc_screen));
    let old_bmp = SelectObject(mem_dc, OVERLAY_DIB.0.into());

    let len = (w * h) as usize;
    let pixels_u32 = std::slice::from_raw_parts_mut(OVERLAY_DIB_BITS, len);

    // BitBlt frozen capture into DIB (hardware-accelerated, same approach as normal mode).
    let has_frozen = {
        let guard = GESTURE_CAPTURE.lock().unwrap();
        if let Some(cap) = guard.as_ref() {
            let hdc_src = CreateCompatibleDC(Some(mem_dc));
            let old = SelectObject(hdc_src, cap.hbitmap.into());
            let _ = BitBlt(mem_dc, 0, 0, w, h, Some(hdc_src), 0, 0, SRCCOPY);
            SelectObject(hdc_src, old);
            let _ = DeleteDC(hdc_src);
            true
        } else {
            false
        }
    };

    let dim_alpha = CURRENT_DIM_ALPHA;
    let is_dragging = RIGHT_DOWN.load(Ordering::SeqCst);

    if has_frozen && dim_alpha > 0 {
        if is_dragging {
            let s_x = START_X.load(Ordering::SeqCst);
            let s_y = START_Y.load(Ordering::SeqCst);
            let l_x = LAST_X.load(Ordering::SeqCst);
            let l_y = LAST_Y.load(Ordering::SeqCst);

            let left = s_x.min(l_x) - sx;
            let top = s_y.min(l_y) - sy;
            let right = s_x.max(l_x) - sx;
            let bottom = s_y.max(l_y) - sy;

            if (right - left).abs() > 0 && (bottom - top).abs() > 0 {
                render_frozen_with_selection(pixels_u32, w, h, left, top, right, bottom, dim_alpha);
            } else {
                dim_pixels(pixels_u32, 256u32 - dim_alpha as u32);
            }
        } else {
            // Frozen frame with dim only (fade-in before drag starts, or shouldn't happen)
            dim_pixels(pixels_u32, 256u32 - dim_alpha as u32);
        }
    } else {
        // No frozen frame: transparent overlay (original behavior)
        if CURRENT_DIM_ALPHA > 0 {
            let alpha = CURRENT_DIM_ALPHA as u32;
            pixels_u32.fill(alpha << 24);
        } else {
            pixels_u32.fill(0);
        }

        if RIGHT_DOWN.load(Ordering::SeqCst) {
            let s_x = START_X.load(Ordering::SeqCst);
            let s_y = START_Y.load(Ordering::SeqCst);
            let l_x = LAST_X.load(Ordering::SeqCst);
            let l_y = LAST_Y.load(Ordering::SeqCst);

            let left = s_x.min(l_x) - sx;
            let top = s_y.min(l_y) - sy;
            let right = s_x.max(l_x) - sx;
            let bottom = s_y.max(l_y) - sy;

            if (right - left).abs() > 0 && (bottom - top).abs() > 0 && CURRENT_DIM_ALPHA > 0 {
                let clear_l = left.max(0).min(w);
                let clear_r = right.max(0).min(w);
                let clear_t = top.max(0).min(h);
                let clear_b = bottom.max(0).min(h);

                for y in clear_t..clear_b {
                    let start = (y * w + clear_l) as usize;
                    let end = (y * w + clear_r) as usize;
                    if start < end && end <= pixels_u32.len() {
                        pixels_u32[start..end].fill(0);
                    }
                }

                let default_radius = 8.0f32;
                let border_width = 2.0f32;
                let sel_hw = (right - left) as f32 / 2.0;
                let sel_hh = (bottom - top) as f32 / 2.0;
                let cx = left as f32 + sel_hw;
                let cy = top as f32 + sel_hh;
                let radius = default_radius.min(sel_hw).min(sel_hh);

                let b_left = (left - 10).max(0);
                let b_top = (top - 10).max(0);
                let b_right = (right + 10).min(w);
                let b_bottom = (bottom + 10).min(h);
                let rad_int = radius.ceil() as i32;
                let top_band_end = (top + rad_int).min(b_bottom);
                let bottom_band_start = (bottom - rad_int).max(top_band_end);

                for py_int in b_top..b_bottom {
                    let row_base = (py_int * w) as usize;
                    if py_int >= top_band_end && py_int < bottom_band_start {
                        let lb = left as usize;
                        let rb = right as usize;
                        if row_base + lb < pixels_u32.len() {
                            for x in 0..2 {
                                if row_base + lb + x < pixels_u32.len() {
                                    pixels_u32[row_base + lb + x] = 0xFFFFFFFF;
                                }
                                if row_base + rb - 1 - x < pixels_u32.len() {
                                    pixels_u32[row_base + rb - 1 - x] = 0xFFFFFFFF;
                                }
                            }
                        }
                        continue;
                    }
                    let py = py_int as f32 + 0.5;
                    for px_int in b_left..b_right {
                        let idx = row_base + px_int as usize;
                        if idx >= pixels_u32.len() {
                            continue;
                        }
                        let px = px_int as f32 + 0.5;
                        let dx = (px - cx).abs() - (sel_hw - radius);
                        let dy = (py - cy).abs() - (sel_hh - radius);
                        let dist = if dx > 0.0 && dy > 0.0 {
                            (dx * dx + dy * dy).sqrt() - radius
                        } else {
                            dx.max(dy) - radius
                        };
                        let alpha_outer = (0.5 - dist).clamp(0.0, 1.0);
                        let alpha_inner = (0.5 - (dist + border_width)).clamp(0.0, 1.0);
                        let border_alpha = alpha_outer - alpha_inner;
                        if border_alpha > 0.001 {
                            let a = (border_alpha * 255.0) as u32;
                            let c = (border_alpha * 255.0) as u32;
                            pixels_u32[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                        }
                    }
                }
            }
        }
    }

    // 3. Update the Layered Window
    let pt_src = POINT { x: 0, y: 0 };
    let size = SIZE { cx: w, cy: h };
    let pt_dst = POINT { x: sx, y: sy };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let _ = UpdateLayeredWindow(
        hwnd,
        Some(hdc_screen),
        Some(&pt_dst),
        Some(&size),
        Some(mem_dc),
        Some(&pt_src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );

    // Cleanup DC only (bitmap is cached and reused)
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, hdc_screen);
}}

unsafe extern "system" fn rect_overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT { unsafe {
    match msg {
        WM_TIMER => {
            if wparam.0 == DIM_TIMER_ID {
                let target = if RIGHT_DOWN.load(Ordering::SeqCst) {
                    TARGET_DIM_ALPHA
                } else {
                    0
                };
                let mut changed = false;
                if CURRENT_DIM_ALPHA < target {
                    CURRENT_DIM_ALPHA = CURRENT_DIM_ALPHA.saturating_add(DIM_FADE_STEP).min(target);
                    changed = true;
                } else if CURRENT_DIM_ALPHA > target {
                    CURRENT_DIM_ALPHA = CURRENT_DIM_ALPHA.saturating_sub(DIM_FADE_STEP).max(target);
                    changed = true;
                }

                if changed || RIGHT_DOWN.load(Ordering::SeqCst) {
                    // Render during fade animation AND during drag.
                    // Timer at 60fps reads latest mouse position from atomics.
                    sync_rect_overlay(hwnd);
                } else {
                    // Fade complete, not dragging — clean up
                    let _ = KillTimer(Some(hwnd), DIM_TIMER_ID);
                    // Restore click-through if it was removed during drag
                    let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    if style & WS_EX_TRANSPARENT.0 as i32 == 0 {
                        SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT.0 as i32);
                    }
                    // Clear any leftover capture
                    *GESTURE_CAPTURE.lock().unwrap() = None;
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}}
