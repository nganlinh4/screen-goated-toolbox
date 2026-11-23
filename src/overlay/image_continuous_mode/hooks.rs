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
        if code < 0 || !RUNNING.load(Ordering::SeqCst) {
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
                // Other hooks may also call exit() for the same keystroke; the
                // cancelled gesture remains pending until the button is released.
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
        if code < 0 || !RUNNING.load(Ordering::SeqCst) {
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

                // Admission and stop share the lifecycle lock: once a down is
                // swallowed, stop first cancels the gesture and leaves its
                // owner alive to consume the matching release.
                {
                    let state = LIFECYCLE.lock().unwrap();
                    match state.begin_gesture(&GESTURE) {
                        None => return CallNextHookEx(None, code, wparam, lparam),
                        Some(false) => return LRESULT(1),
                        Some(true) => {}
                    }
                }
                START_X.store(pt.x, Ordering::SeqCst);
                START_Y.store(pt.y, Ordering::SeqCst);
                LAST_X.store(pt.x, Ordering::SeqCst);
                LAST_Y.store(pt.y, Ordering::SeqCst);
                // The hook only records the gesture; capture runs after it returns.
                defer(
                    GESTURE_BEGIN_MESSAGE,
                    WPARAM(0),
                    LPARAM(0),
                    begin_gesture_capture,
                );
                return LRESULT(1); // Swallow event
            }

            WM_MOUSEMOVE => {
                if GESTURE.is_dragging() {
                    LAST_X.store(pt.x, Ordering::SeqCst);
                    LAST_Y.store(pt.y, Ordering::SeqCst);
                    let hwnd_value = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
                    if hwnd_value != 0
                        && !DRAG_RENDER_PENDING.swap(true, Ordering::SeqCst)
                        && PostMessageW(
                            Some(HWND(hwnd_value as *mut _)),
                            DRAG_RENDER_MESSAGE,
                            WPARAM(0),
                            LPARAM(0),
                        )
                        .is_err()
                    {
                        DRAG_RENDER_PENDING.store(false, Ordering::SeqCst);
                    }
                }
            }

            WM_RBUTTONUP => {
                match GESTURE.release() {
                    gesture::Release::Dragging => {
                        DRAG_RENDER_PENDING.store(false, Ordering::SeqCst);
                        // Crop and dispatch run after the hook returns. The start
                        // travels with the message so a newer press cannot move it.
                        let start = POINT {
                            x: START_X.load(Ordering::SeqCst),
                            y: START_Y.load(Ordering::SeqCst),
                        };
                        defer(
                            GESTURE_END_MESSAGE,
                            WPARAM(pack_point(start) as usize),
                            LPARAM(pack_point(pt) as isize),
                            finish_gesture,
                        );
                        return LRESULT(1);
                    }
                    gesture::Release::Cancelled => {
                        restore_cancelled_gesture();
                        return LRESULT(1);
                    }
                    gesture::Release::Idle => {}
                }
            }

            WM_MOUSEWHEEL if GESTURE.is_dragging() => {
                let delta = (mouse_struct.mouseData >> 16) as i16;
                defer(
                    GESTURE_ZOOM_MESSAGE,
                    WPARAM(delta as u16 as usize),
                    LPARAM(pack_point(pt) as isize),
                    zoom_gesture,
                );
                return LRESULT(1);
            }

            _ => {}
        }

        CallNextHookEx(None, code, wparam, lparam)
    }
}

const _: () = assert!(std::mem::size_of::<usize>() == 8);

fn pack_point(point: POINT) -> u64 {
    (u64::from(point.x as u32) << 32) | u64::from(point.y as u32)
}

fn unpack_point(packed: u64) -> POINT {
    POINT {
        x: (packed >> 32) as u32 as i32,
        y: packed as u32 as i32,
    }
}

/// Queues gesture work for the overlay window, whose thread also runs these
/// hooks, so it executes after the hook returns and in input order. Without a
/// window it runs inline.
unsafe fn defer(message: u32, wparam: WPARAM, lparam: LPARAM, inline: unsafe fn(WPARAM, LPARAM)) {
    let hwnd_value = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
    let posted = hwnd_value != 0
        && unsafe { PostMessageW(Some(HWND(hwnd_value as *mut _)), message, wparam, lparam) }
            .is_ok();
    if !posted {
        unsafe { inline(wparam, lparam) };
    }
}

/// Handles deferred gesture messages; false for any other message.
pub(super) unsafe fn handle_deferred(message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let handler: unsafe fn(WPARAM, LPARAM) = match message {
        GESTURE_BEGIN_MESSAGE => begin_gesture_capture,
        GESTURE_END_MESSAGE => finish_gesture,
        GESTURE_ZOOM_MESSAGE => zoom_gesture,
        _ => return false,
    };
    unsafe { handler(wparam, lparam) };
    true
}

unsafe fn begin_gesture_capture(_: WPARAM, _: LPARAM) {
    if !GESTURE.is_dragging() {
        return;
    }
    unsafe {
        // A new gesture must not inherit magnification from a cancelled one.
        let destination =
            crate::overlay::process::chain::AutoPasteDestination::capture_foreground();
        *GESTURE_CAPTURE.lock().unwrap() = None;
        logic::reset_magnification();

        // Hide all badges BEFORE capture so they never appear in the screenshot.
        crate::overlay::text_selection::hide_all_badges_for_capture();
        match graphics::capture_screen_now() {
            Ok(capture) => {
                *GESTURE_CAPTURE.lock().unwrap() = Some(CapturedGesture {
                    image: capture,
                    destination,
                });
            }
            Err(err) => {
                crate::log_info!("[ImageContinuous] Capture Error: {err}");
            }
        }
        crate::overlay::text_selection::restore_badges_after_capture();

        // Trigger fade-in animation
        let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
        if hwnd_val != 0 {
            let hwnd = HWND(hwnd_val as *mut _);
            SetTimer(Some(hwnd), DIM_TIMER_ID, 16, None);

            // Remove WS_EX_TRANSPARENT so the overlay shows the crosshair cursor.
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, style & !(WS_EX_TRANSPARENT.0 as i32));
            if let Ok(cursor) = LoadCursorW(None, IDC_CROSS) {
                SetCursor(Some(cursor));
            }
        }
    }
}

unsafe fn finish_gesture(wparam: WPARAM, lparam: LPARAM) {
    let start = unpack_point(wparam.0 as u64);
    let end = unpack_point(lparam.0 as u64);
    unsafe {
        // Trigger fade-out animation
        let hwnd_val = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
        if hwnd_val != 0 {
            SetTimer(Some(HWND(hwnd_val as *mut _)), DIM_TIMER_ID, 16, None);
        }

        logic::reset_magnification();

        if (end.x - start.x).abs() <= 5 && (end.y - start.y).abs() <= 5 {
            logic::handle_color_pick(end);
        } else {
            logic::handle_region_capture(start.x, start.y, end.x, end.y);
        }

        // Restore WS_EX_TRANSPARENT for click-through behavior
        if hwnd_val != 0 {
            let hwnd = HWND(hwnd_val as *mut _);
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT.0 as i32);
        }

        // Clean up capture (fade-out uses transparent overlay)
        *GESTURE_CAPTURE.lock().unwrap() = None;
    }
}

unsafe fn zoom_gesture(wparam: WPARAM, lparam: LPARAM) {
    if GESTURE.is_dragging() {
        logic::handle_zoom(
            i32::from(wparam.0 as u16 as i16),
            unpack_point(lparam.0 as u64),
        );
    }
}

/// Runs on the overlay thread, after cancellation, without touching a newer drag.
pub(super) unsafe fn restore_cancelled_gesture() {
    if GESTURE.is_dragging() {
        return;
    }
    unsafe {
        logic::reset_magnification();
        *GESTURE_CAPTURE.lock().unwrap() = None;
        let hwnd_value = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
        if hwnd_value != 0 {
            let hwnd = HWND(hwnd_value as *mut _);
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT.0 as i32);
            graphics::sync_rect_overlay(hwnd);
            SetTimer(Some(hwnd), DIM_TIMER_ID, 16, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{pack_point, unpack_point};
    use windows::Win32::Foundation::POINT;

    #[test]
    fn deferred_gesture_points_keep_negative_virtual_screen_coordinates() {
        for (x, y) in [(0, 0), (-1920, -1080), (7679, 4319), (i32::MIN, i32::MAX)] {
            let point = unpack_point(pack_point(POINT { x, y }));
            assert_eq!((point.x, point.y), (x, y));
        }
        let wheel: i16 = -120;
        assert_eq!(i32::from(wheel as u16 as usize as u16 as i16), -120);
    }
}
