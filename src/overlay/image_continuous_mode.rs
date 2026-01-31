//! Image Continuous Mode
//!
//! A non-blocking image selection mode that allows users to work normally while
//! being able to capture screen regions using right-click/drag gestures.

use crate::overlay::process::start_processing_pipeline;
use crate::overlay::selection::extract_crop_from_hbitmap_public;
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

    IS_ACTIVE.store(true, Ordering::SeqCst);

    // Clear any "recently cancelled" state from previous sessions
    // This allows text badges to be shown fresh in this new image continuous session
    crate::overlay::text_selection::clear_recently_cancelled();

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
    crate::log_info!("[ImageContinuous] Mode exit() called");

    IS_ACTIVE.store(false, Ordering::SeqCst);

    // Hide badge
    crate::overlay::text_selection::set_image_continuous_badge(false);

    // NOTE: We NO LONGER call crate::overlay::continuous_mode::deactivate() here.
    // This allows image continuous mode to exist independently of other modes.

    // Signal the thread to exit by posting WM_QUIT
    let thread_id = OVERLAY_THREAD_ID.load(Ordering::SeqCst);
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }

    crate::log_info!("[ImageContinuous] Mode exited (Quit signal posted)");
}

pub fn get_hotkey_name() -> String {
    HOTKEY_NAME.lock().unwrap().clone()
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

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(rect_overlay_wnd_proc);
        wc.hInstance = instance.into();
        wc.lpszClassName = class_name;
        wc.style = CS_HREDRAW | CS_VREDRAW;
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
            TranslateMessage(&msg);
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

        // Clear capture
        *GESTURE_CAPTURE.lock().unwrap() = None;
        RECT_OVERLAY_HWND.store(0, Ordering::SeqCst);
        OVERLAY_THREAD_ID.store(0, Ordering::SeqCst);
    }
}

// === HOOK PROCS ===

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
            // Only exit on REAL user-pressed ESC, not injected events
            if is_injected {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            exit();
            return LRESULT(1);
        }
    } else if wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize {
        if vk == TRIGGER_VK.load(Ordering::SeqCst) {
            HAS_RELEASED_SINCE_ACTIVATION.store(true, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
            crate::overlay::text_selection::hide_all_badges_for_capture();
            
            // Small delay to ensure window is hidden before capture
            std::thread::sleep(std::time::Duration::from_millis(16));

            // Capture screen NOW at start of drag
            // This ensures we get what the user sees before drawing box
            if let Ok(capture) = capture_screen_now() {
                *GESTURE_CAPTURE.lock().unwrap() = Some(capture);
            }
            
            // Restore badges after capture is complete
            crate::overlay::text_selection::restore_badges_after_capture();

            // Trigger redraw of overlay (it will draw the box now that RIGHT_DOWN is true)
            let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                let hwnd = HWND(hwnd_val as *mut _);
                // Force sync
                sync_rect_overlay(hwnd);
            }

            return LRESULT(1); // Swallow event
        }

        WM_MOUSEMOVE => {
            if RIGHT_DOWN.load(Ordering::SeqCst) {
                LAST_X.store(pt.x, Ordering::SeqCst);
                LAST_Y.store(pt.y, Ordering::SeqCst);

                // Update overlay
                let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
                if hwnd_val != 0 {
                    sync_rect_overlay(HWND(hwnd_val as *mut _));
                }
            }
        }

        WM_RBUTTONUP => {
            if RIGHT_DOWN.load(Ordering::SeqCst) {
                RIGHT_DOWN.store(false, Ordering::SeqCst);

                // Clear overlay
                let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
                if hwnd_val != 0 {
                    sync_rect_overlay(HWND(hwnd_val as *mut _));
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

                // Clean up capture
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
}

// === LOGIC ===

unsafe fn handle_color_pick(pt: POINT) {
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
}

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
        if !MAG_DLL_LOADED {
            if let Ok(lib) = LoadLibraryW(windows::core::w!("Magnification.dll")) {
                if let Some(init) = GetProcAddress(lib, windows::core::s!("MagInitialize")) {
                    MAG_INITIALIZE_FN = Some(std::mem::transmute(init));
                    if let Some(f) = MAG_INITIALIZE_FN {
                        f();
                    }
                }
                if let Some(u) = GetProcAddress(lib, windows::core::s!("MagUninitialize")) {
                    MAG_UNINITIALIZE_FN = Some(std::mem::transmute(u));
                }
                if let Some(s) = GetProcAddress(lib, windows::core::s!("MagSetFullscreenTransform"))
                {
                    MAG_SET_FULLSCREEN_TRANSFORM_FN = Some(std::mem::transmute(s));
                }
                MAG_DLL_LOADED = true;
                MAG_INITIALIZED.store(true, Ordering::SeqCst);
            }
        }
    }
}

// === GRAPHICS ===

unsafe fn capture_screen_now() -> anyhow::Result<GdiCapture> {
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
}

unsafe fn sync_rect_overlay(hwnd: HWND) {
    let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);

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
    let mut bits: *mut u32 = std::ptr::null_mut();
    let hbm = CreateDIBSection(
        Some(hdc_screen),
        &bmi,
        DIB_RGB_COLORS,
        &mut bits as *mut _ as *mut _,
        None,
        0,
    )
    .unwrap();

    let mem_dc = CreateCompatibleDC(Some(hdc_screen));
    let old_bmp = SelectObject(mem_dc, hbm.into());

    // Clear to transparent
    let len = (w * h) as usize;
    std::ptr::write_bytes(bits, 0, len);

    // Draw box if dragging - using white rounded SDF corners (matching normal image selection mode)
    if RIGHT_DOWN.load(Ordering::SeqCst) {
        let s_x = START_X.load(Ordering::SeqCst);
        let s_y = START_Y.load(Ordering::SeqCst);
        let l_x = LAST_X.load(Ordering::SeqCst);
        let l_y = LAST_Y.load(Ordering::SeqCst);

        let left = s_x.min(l_x) - sx;
        let top = s_y.min(l_y) - sy;
        let right = s_x.max(l_x) - sx;
        let bottom = s_y.max(l_y) - sy;

        let rect_w = (right - left).abs();
        let rect_h = (bottom - top).abs();

        if rect_w > 0 && rect_h > 0 {
            let pixels = std::slice::from_raw_parts_mut(bits, len);

            // ANTI-ALIASED ROUNDED BOX (matching selection.rs styling)
            let default_radius = 12.0f32;
            let border_width = 2.0f32;

            // Box coordinates
            let l_f = left as f32;
            let t_f = top as f32;
            let r_f = right as f32;
            let b_f = bottom as f32;

            let hw = (r_f - l_f) / 2.0;
            let hh = (b_f - t_f) / 2.0;
            let cx = l_f + hw;
            let cy = t_f + hh;

            // ADAPTIVE RADIUS: Scale down if box is smaller than radius
            let radius = default_radius.min(hw).min(hh);

            // Processing bounds with margin for anti-aliasing
            let b_left = (left - 10).max(0);
            let b_top = (top - 10).max(0);
            let b_right = (right + 10).min(w);
            let b_bottom = (bottom + 10).min(h);

            let rad_int = radius.ceil() as i32;
            let top_band_end = (top + rad_int).min(b_bottom);
            let bottom_band_start = (bottom - rad_int).max(top_band_end);

            for py_int in b_top..b_bottom {
                let row_base = (py_int * w) as usize;

                // --- FAST PATH: Middle Band (no corners, just straight vertical edges) ---
                if py_int >= top_band_end && py_int < bottom_band_start {
                    let lb = left as usize;
                    let rb = right as usize;
                    if row_base + lb < pixels.len() {
                        // Draw Left/Right Borders (2 pixels wide, opaque white)
                        for x in 0..2 {
                            if row_base + lb + x < pixels.len() {
                                pixels[row_base + lb + x] = 0xFFFFFFFF;
                            }
                            if row_base + rb - 1 - x < pixels.len() {
                                pixels[row_base + rb - 1 - x] = 0xFFFFFFFF;
                            }
                        }
                    }
                    continue;
                }

                // --- SLOW PATH: Top/Bottom Bands (SDF for corners) ---
                let py = py_int as f32 + 0.5; // pixel center
                for px_int in b_left..b_right {
                    let idx = row_base + px_int as usize;
                    if idx >= pixels.len() {
                        continue;
                    }

                    let px = px_int as f32 + 0.5;

                    // Rounded Rect SDF (Signed Distance Field)
                    let dx = (px - cx).abs() - (hw - radius);
                    let dy = (py - cy).abs() - (hh - radius);

                    let dist = if dx > 0.0 && dy > 0.0 {
                        (dx * dx + dy * dy).sqrt() - radius
                    } else {
                        dx.max(dy) - radius
                    };

                    // Composition: Only draw the border (no background dimming for continuous mode)
                    let alpha_outer = (0.5 - dist).clamp(0.0, 1.0);
                    let alpha_inner = (0.5 - (dist + border_width)).clamp(0.0, 1.0);
                    let border_alpha = alpha_outer - alpha_inner;

                    if border_alpha > 0.001 {
                        // Final Color (Pre-multiplied alpha white)
                        let a = (border_alpha * 255.0) as u32;
                        let c = (border_alpha * 255.0) as u32;
                        pixels[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                    }
                }
            }
        }
    }

    let pt_src = POINT { x: 0, y: 0 };
    let size = SIZE { cx: w, cy: h };
    let pt_dst = POINT { x: sx, y: sy };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
        ..Default::default()
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

    let _ = SelectObject(mem_dc, old_bmp);
    let _ = DeleteDC(mem_dc);
    let _ = DeleteObject(hbm.into());
    let _ = ReleaseDC(None, hdc_screen);
}

unsafe extern "system" fn rect_overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
