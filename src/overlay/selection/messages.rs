// --- SELECTION MESSAGES ---
// Window procedure and keyboard hook for selection overlay.

use super::magnification::load_magnification_api;
use super::render::{extract_crop_from_hbitmap, sync_layered_window_contents};
use super::state::*;
use crate::overlay::process::start_processing_pipeline;
use crate::win_types::SendHbitmap;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub unsafe extern "system" fn selection_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize {
            if kbd.vkCode == VK_ESCAPE.0 as u32 {
                crate::overlay::continuous_mode::deactivate();
                SELECTION_ABORT_SIGNAL.store(true, Ordering::SeqCst);
                let hwnd = std::ptr::addr_of!(SELECTION_OVERLAY_HWND).read().0;
                if !hwnd.is_invalid() {
                    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                }
                return LRESULT(1);
            }
            if kbd.vkCode == TRIGGER_VK_CODE {
                if !IS_HOTKEY_HELD.load(Ordering::SeqCst) {
                    crate::overlay::continuous_mode::deactivate();
                    SELECTION_ABORT_SIGNAL.store(true, Ordering::SeqCst);
                    let hwnd = std::ptr::addr_of!(SELECTION_OVERLAY_HWND).read().0;
                    if !hwnd.is_invalid() {
                        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                    }
                    return LRESULT(1);
                }
            }
        } else if wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize {
            if kbd.vkCode == TRIGGER_VK_CODE {
                IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

#[allow(static_mut_refs)]
pub unsafe extern "system" fn selection_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            if !IS_FADING_OUT {
                IS_DRAGGING = true;
                let _ = GetCursorPos(std::ptr::addr_of_mut!(START_POS));
                CURR_POS = START_POS;
                SetCapture(hwnd);
                sync_layered_window_contents(hwnd);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            if !IS_FADING_OUT && ZOOM_LEVEL > 1.0 {
                IS_RIGHT_DRAGGING = true;
                let _ = GetCursorPos(std::ptr::addr_of_mut!(LAST_PAN_POS));
                SetCapture(hwnd);
                let _ = SetTimer(Some(hwnd), ZOOM_TIMER_ID, 16, None);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if IS_RIGHT_DRAGGING {
                IS_RIGHT_DRAGGING = false;
                let _ = ReleaseCapture();
            }
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTCLIENT as _),
        WM_MOUSEMOVE => {
            if IS_DRAGGING {
                let _ = GetCursorPos(std::ptr::addr_of_mut!(CURR_POS));
                sync_layered_window_contents(hwnd);
            } else if IS_RIGHT_DRAGGING {
                let mut curr_pan = POINT::default();
                let _ = GetCursorPos(&mut curr_pan);

                let dx_screen = curr_pan.x - LAST_PAN_POS.x;
                let dy_screen = curr_pan.y - LAST_PAN_POS.y;
                LAST_PAN_POS = curr_pan;

                if RENDER_ZOOM > 0.1 {
                    let sensitivity = 2.0;
                    let dx_source = (dx_screen as f32 / RENDER_ZOOM) * sensitivity;
                    let dy_source = (dy_screen as f32 / RENDER_ZOOM) * sensitivity;

                    ZOOM_CENTER_X -= dx_source;
                    ZOOM_CENTER_Y -= dy_source;
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if !IS_FADING_OUT && !IS_DRAGGING {
                let delta = ((wparam.0 >> 16) as i16) as i32;
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);

                if delta > 0 {
                    ZOOM_LEVEL = (ZOOM_LEVEL + ZOOM_STEP).min(MAX_ZOOM);
                    ZOOM_CENTER_X = cursor.x as f32;
                    ZOOM_CENTER_Y = cursor.y as f32;
                } else if delta < 0 {
                    ZOOM_LEVEL = (ZOOM_LEVEL - ZOOM_STEP).max(MIN_ZOOM);
                }

                if RENDER_CENTER_X == 0.0 && RENDER_CENTER_Y == 0.0 {
                    RENDER_CENTER_X = ZOOM_CENTER_X;
                    RENDER_CENTER_Y = ZOOM_CENTER_Y;
                }

                if !MAG_INITIALIZED && ZOOM_LEVEL > 1.0 {
                    if load_magnification_api() {
                        if let Some(init_fn) = MAG_INITIALIZE {
                            if init_fn().as_bool() {
                                MAG_INITIALIZED = true;
                            }
                        }
                    }
                }

                let _ = SetTimer(Some(hwnd), ZOOM_TIMER_ID, 16, None);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if IS_DRAGGING {
                IS_DRAGGING = false;
                let _ = ReleaseCapture();

                let rect = RECT {
                    left: START_POS.x.min(CURR_POS.x),
                    top: START_POS.y.min(CURR_POS.y),
                    right: START_POS.x.max(CURR_POS.x),
                    bottom: START_POS.y.max(CURR_POS.y),
                };

                let width = (rect.right - rect.left).abs();
                let height = (rect.bottom - rect.top).abs();

                if width <= 10 && height <= 10 {
                    // COLOR PICKER
                    handle_color_picker(hwnd);
                    IS_FADING_OUT = true;
                    if MAG_INITIALIZED {
                        if let Some(transform_fn) = MAG_SET_FULLSCREEN_TRANSFORM {
                            let _ = transform_fn(1.0, 0, 0);
                        }
                    }
                    let _ = SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
                    return LRESULT(0);
                }

                if width > 10 && height > 10 {
                    // Handle selection
                    if let Some(result) = handle_selection(hwnd, rect) {
                        return result;
                    }
                } else {
                    let _ = SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            handle_timer(hwnd, wparam.0);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            sync_layered_window_contents(hwnd);
            let _ = EndPaint(hwnd, &mut ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            if !IS_FADING_OUT {
                IS_FADING_OUT = true;
                if MAG_INITIALIZED {
                    if let Some(transform_fn) = MAG_SET_FULLSCREEN_TRANSFORM {
                        let _ = transform_fn(1.0, 0, 0);
                    }
                }
                let _ = KillTimer(Some(hwnd), FADE_TIMER_ID);
                let _ = KillTimer(Some(hwnd), ZOOM_TIMER_ID);
                SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // Reset magnification before closing
            if MAG_INITIALIZED {
                if let Some(transform_fn) = MAG_SET_FULLSCREEN_TRANSFORM {
                    let _ = transform_fn(1.0, 0, 0);
                }
                if let Some(uninit_fn) = MAG_UNINITIALIZE {
                    let _ = uninit_fn();
                }
                MAG_INITIALIZED = false;
            }

            // Cleanup cached back buffer resources
            if !std::ptr::addr_of!(CACHED_BITMAP).read().is_invalid() {
                let _ = DeleteObject(CACHED_BITMAP.0.into());
                CACHED_BITMAP = SendHbitmap::default();
                CACHED_BITS = std::ptr::null_mut();
            }
            CACHED_W = 0;
            CACHED_H = 0;

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Handle color picker click
unsafe fn handle_color_picker(hwnd: HWND) {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let hex_color = {
        let guard = APP.lock().unwrap();
        if let Some(capture) = &guard.screenshot_handle {
            let hdc_screen = GetDC(None);
            let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
            let old_bmp = SelectObject(hdc_mem, capture.hbitmap.into());

            let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let local_x = pt.x - sx;
            let local_y = pt.y - sy;

            let color = GetPixel(hdc_mem, local_x, local_y);

            SelectObject(hdc_mem, old_bmp);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);

            let r = (color.0 & 0x000000FF) as u8;
            let g = ((color.0 & 0x0000FF00) >> 8) as u8;
            let b = ((color.0 & 0x00FF0000) >> 16) as u8;

            Some(format!("#{:02X}{:02X}{:02X}", r, g, b))
        } else {
            None
        }
    };

    if let Some(hex) = hex_color {
        crate::overlay::utils::copy_to_clipboard(&hex, hwnd);
        crate::overlay::auto_copy_badge::show_auto_copy_badge_text(&hex);
    }
}

/// Handle selection completion
#[allow(static_mut_refs)]
unsafe fn handle_selection(hwnd: HWND, rect: RECT) -> Option<LRESULT> {
    // Check if this is a MASTER preset
    let is_master = {
        let guard = APP.lock().unwrap();
        guard
            .config
            .presets
            .get(CURRENT_PRESET_IDX)
            .map(|p| p.is_master)
            .unwrap_or(false)
    };

    // For MASTER presets, show the preset wheel first
    let final_preset_idx = if is_master {
        let mut cursor_pos = POINT::default();
        let _ = GetCursorPos(&mut cursor_pos);

        ZOOM_ALPHA_OVERRIDE = Some(60);
        sync_layered_window_contents(hwnd);

        let selected = crate::overlay::preset_wheel::show_preset_wheel("image", None, cursor_pos);

        if let Some(idx) = selected {
            Some(idx)
        } else {
            IS_FADING_OUT = true;
            SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
            return Some(LRESULT(0));
        }
    } else {
        Some(CURRENT_PRESET_IDX)
    };

    if let Some(preset_idx) = final_preset_idx {
        // CHECK FOR CONTINUOUS MODE ACTIVATION
        let is_held = {
            if TRIGGER_VK_CODE != 0 {
                (GetAsyncKeyState(TRIGGER_VK_CODE as i32) as u16 & 0x8000) != 0
            } else {
                false
            }
        };

        let held_detected = HOLD_DETECTED_THIS_SESSION.load(Ordering::SeqCst);

        if (is_held || held_detected) && !CONTINUOUS_ACTIVATED_THIS_SESSION.load(Ordering::SeqCst) {
            let mut hotkey_name = crate::overlay::continuous_mode::get_hotkey_name();
            if hotkey_name.is_empty() {
                hotkey_name = crate::overlay::continuous_mode::get_latest_hotkey_name();
            }
            if hotkey_name.is_empty() {
                hotkey_name = "Hotkey".to_string();
            }

            crate::overlay::image_continuous_mode::enter(
                preset_idx,
                hotkey_name.clone(),
                CURRENT_HOTKEY_ID,
            );

            IS_FADING_OUT = true;
            let _ = SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
        }

        // EXTRACT CROP
        let (cropped_img, config, preset) = {
            let mut guard = APP.lock().unwrap();
            guard.config.active_preset_idx = preset_idx;

            let capture = guard
                .screenshot_handle
                .as_ref()
                .expect("Screenshot handle missing");
            let config_clone = guard.config.clone();
            let preset_clone = guard.config.presets[preset_idx].clone();

            let img = extract_crop_from_hbitmap(capture, rect);

            (img, config_clone, preset_clone)
        };

        // TRIGGER PROCESSING
        std::thread::spawn(move || {
            start_processing_pipeline(cropped_img, rect, config, preset);
        });

        // CHECK IF NEW MODE IS ACTIVE
        if crate::overlay::image_continuous_mode::is_active() {
            IS_FADING_OUT = true;
            let _ = SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
            return Some(LRESULT(0));
        }

        if crate::overlay::continuous_mode::is_active() {
            START_POS = POINT::default();
            CURR_POS = POINT::default();
            ZOOM_ALPHA_OVERRIDE = None;
            sync_layered_window_contents(hwnd);
            return Some(LRESULT(0));
        }

        // START FADE OUT
        IS_FADING_OUT = true;
        if MAG_INITIALIZED {
            if let Some(transform_fn) = MAG_SET_FULLSCREEN_TRANSFORM {
                let _ = transform_fn(1.0, 0, 0);
            }
        }
        let _ = SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);

        return Some(LRESULT(0));
    }

    None
}

/// Handle timer events
#[allow(static_mut_refs)]
unsafe fn handle_timer(hwnd: HWND, timer_id: usize) {
    if timer_id == ZOOM_TIMER_ID {
        handle_zoom_timer(hwnd);
    } else if timer_id == CONTINUOUS_CHECK_TIMER_ID {
        handle_continuous_check_timer(hwnd);
    } else if timer_id == FADE_TIMER_ID {
        handle_fade_timer(hwnd);
    }
}

unsafe fn handle_zoom_timer(hwnd: HWND) {
    let t = 0.4;
    let mut changed = false;

    // Interpolate Zoom
    let diff_zoom = ZOOM_LEVEL - RENDER_ZOOM;
    if diff_zoom.abs() > 0.001 {
        RENDER_ZOOM += diff_zoom * t;
        changed = true;
    } else {
        RENDER_ZOOM = ZOOM_LEVEL;
    }

    // Interpolate Center
    let dx = ZOOM_CENTER_X - RENDER_CENTER_X;
    let dy = ZOOM_CENTER_Y - RENDER_CENTER_Y;

    if dx.abs() > 0.1 || dy.abs() > 0.1 {
        RENDER_CENTER_X += dx * t;
        RENDER_CENTER_Y += dy * t;
        changed = true;
    } else {
        RENDER_CENTER_X = ZOOM_CENTER_X;
        RENDER_CENTER_Y = ZOOM_CENTER_Y;
    }

    // Apply Transform if Changed or Dragging
    if changed || IS_RIGHT_DRAGGING {
        if MAG_INITIALIZED {
            if let Some(transform_fn) = MAG_SET_FULLSCREEN_TRANSFORM {
                if RENDER_ZOOM > 1.01 {
                    let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                    let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
                    let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                    let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

                    let view_w = screen_w as f32 / RENDER_ZOOM;
                    let view_h = screen_h as f32 / RENDER_ZOOM;

                    let mut off_x = RENDER_CENTER_X - view_w / 2.0;
                    let mut off_y = RENDER_CENTER_Y - view_h / 2.0;

                    off_x = off_x
                        .max(screen_x as f32)
                        .min((screen_x + screen_w) as f32 - view_w);
                    off_y = off_y
                        .max(screen_y as f32)
                        .min((screen_y + screen_h) as f32 - view_h);

                    let _ = transform_fn(RENDER_ZOOM, off_x as i32, off_y as i32);
                } else {
                    let _ = transform_fn(1.0, 0, 0);
                }
            }
        }
        sync_layered_window_contents(hwnd);
    } else if !changed && !IS_RIGHT_DRAGGING {
        let _ = KillTimer(Some(hwnd), ZOOM_TIMER_ID);
    }
}

#[allow(static_mut_refs)]
unsafe fn handle_continuous_check_timer(hwnd: HWND) {
    // SYNC PHYSICAL KEY STATE
    if TRIGGER_VK_CODE != 0 {
        let is_physically_down = (GetAsyncKeyState(TRIGGER_VK_CODE as i32) as u16 & 0x8000) != 0;
        if !is_physically_down && IS_HOTKEY_HELD.load(Ordering::SeqCst) {
            IS_HOTKEY_HELD.store(false, Ordering::SeqCst);
        }
    }

    // Background Hold Detection
    if !CONTINUOUS_ACTIVATED_THIS_SESSION.load(Ordering::SeqCst) {
        let heartbeat = crate::overlay::continuous_mode::was_triggered_recently(1500);
        if heartbeat {
            HOLD_DETECTED_THIS_SESSION.store(true, Ordering::SeqCst);

            let is_master = {
                if let Ok(app) = APP.lock() {
                    app.config
                        .presets
                        .get(CURRENT_PRESET_IDX)
                        .map(|p| p.is_master)
                        .unwrap_or(false)
                } else {
                    false
                }
            };

            if !is_master {
                let mut hotkey_name = crate::overlay::continuous_mode::get_hotkey_name();
                if hotkey_name.is_empty() {
                    hotkey_name = crate::overlay::continuous_mode::get_latest_hotkey_name();
                }
                if hotkey_name.is_empty() {
                    hotkey_name = "Hotkey".to_string();
                }

                crate::overlay::image_continuous_mode::enter(
                    CURRENT_PRESET_IDX,
                    hotkey_name.clone(),
                    CURRENT_HOTKEY_ID,
                );

                IS_FADING_OUT = true;
                let _ = SetTimer(Some(hwnd), FADE_TIMER_ID, 16, None);
            }
        }
    }
}

unsafe fn handle_fade_timer(hwnd: HWND) {
    let mut changed = false;
    if IS_FADING_OUT {
        if CURRENT_ALPHA > FADE_STEP {
            CURRENT_ALPHA -= FADE_STEP;
            changed = true;
        } else {
            CURRENT_ALPHA = 0;
            let _ = KillTimer(Some(hwnd), FADE_TIMER_ID);
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
            return;
        }
    } else {
        if CURRENT_ALPHA < TARGET_OPACITY {
            CURRENT_ALPHA =
                (CURRENT_ALPHA as u16 + FADE_STEP as u16).min(TARGET_OPACITY as u16) as u8;
            changed = true;
        } else {
            let _ = KillTimer(Some(hwnd), FADE_TIMER_ID);
        }
    }

    if changed {
        sync_layered_window_contents(hwnd);
    }
}
