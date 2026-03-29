//! Keyboard and mouse hook procedures for image continuous mode.

use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
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
    }
}

pub(super) unsafe extern "system" fn mouse_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
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
                match graphics::capture_screen_now() {
                    Ok(capture) => {
                        *GESTURE_CAPTURE.lock().unwrap() = Some(capture);
                    }
                    Err(err) => {
                        eprintln!("[ImageContinuous] Capture Error: {}", err);
                    }
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

                    logic::reset_magnification();

                    let start_x = START_X.load(Ordering::SeqCst);
                    let start_y = START_Y.load(Ordering::SeqCst);
                    let dx = (pt.x - start_x).abs();
                    let dy = (pt.y - start_y).abs();

                    if dx <= 5 && dy <= 5 {
                        logic::handle_color_pick(pt);
                    } else {
                        logic::handle_region_capture(start_x, start_y, pt.x, pt.y);
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
                    logic::handle_zoom(delta, pt);
                    return LRESULT(1);
                }
            }

            _ => {}
        }

        CallNextHookEx(None, code, wparam, lparam)
    }
}
