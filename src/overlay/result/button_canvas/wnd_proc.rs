//! Window procedure and message handlers for button canvas

use super::{
    get_dpi_scale, theme::get_canvas_theme_css, ACTIVE_DRAG_SNAPSHOT, ACTIVE_DRAG_TARGET,
    CANVAS_WEBVIEW, CURSOR_POLL_TIMER_ID, DRAG_IS_GROUP, LAST_DRAG_POS, LAST_THEME_IS_DARK,
    MARKDOWN_WINDOWS, PENDING_REFINE_UPDATES, START_DRAG_POS, WM_APP_HIDE_CANVAS,
    WM_APP_SEND_REFINE_TEXT, WM_APP_SHOW_CANVAS, WM_APP_UPDATE_WINDOWS,
};
use crate::overlay::result::state::WINDOW_STATES;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::Rect;

pub unsafe extern "system" fn canvas_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_UPDATE_WINDOWS => {
            let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                v_x,
                v_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
            super::window::send_windows_update();
            LRESULT(0)
        }

        WM_APP_SHOW_CANVAS => {
            handle_show_canvas(hwnd);
            LRESULT(0)
        }

        WM_APP_HIDE_CANVAS => {
            CANVAS_WEBVIEW.with(|cell| {
                if let Some(webview) = cell.borrow().as_ref() {
                    let _ = webview.set_visible(false);
                }
            });
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = KillTimer(Some(hwnd), CURSOR_POLL_TIMER_ID);
            LRESULT(0)
        }

        WM_APP_SEND_REFINE_TEXT => {
            handle_send_refine_text(wparam, lparam);
            LRESULT(0)
        }

        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),

        WM_MOUSEMOVE => handle_mouse_move(hwnd, msg, wparam, lparam),

        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => handle_button_up(hwnd, msg, wparam, lparam),

        WM_TIMER => {
            handle_timer(wparam);
            LRESULT(0)
        }

        WM_DISPLAYCHANGE => {
            handle_display_change(hwnd);
            LRESULT(0)
        }

        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn handle_show_canvas(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    let foreground = GetForegroundWindow();

    let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let v_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let v_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), v_x, v_y, v_w, v_h, SWP_NOACTIVATE);

    CANVAS_WEBVIEW.with(|cell| {
        if let Some(webview) = cell.borrow().as_ref() {
            let _ = webview.set_bounds(Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    v_w as u32, v_h as u32,
                )),
            });
            let _ = webview.set_visible(true);
        }
    });

    if !foreground.0.is_null() && IsWindow(Some(foreground)).as_bool() {
        let _ = SetForegroundWindow(foreground);
    }

    let _ = SetTimer(Some(hwnd), CURSOR_POLL_TIMER_ID, 100, None);
}

fn handle_send_refine_text(wparam: WPARAM, lparam: LPARAM) {
    let hwnd_key = wparam.0 as isize;
    let text = {
        let mut updates = PENDING_REFINE_UPDATES.lock().unwrap();
        updates.remove(&hwnd_key)
    };

    if let Some(text) = text {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace("${", "\\${")
            .replace('\r', "");

        let is_insert = lparam.0 != 0;
        let script = format!(
            "if(window.setRefineText) window.setRefineText('{}', `{}`, {});",
            hwnd_key,
            escaped,
            if is_insert { "true" } else { "false" }
        );

        CANVAS_WEBVIEW.with(|cell| {
            if let Some(webview) = cell.borrow().as_ref() {
                let _ = webview.evaluate_script(&script);
            }
        });
    }
}

unsafe fn handle_mouse_move(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let target_val = ACTIVE_DRAG_TARGET.load(Ordering::SeqCst);
    if target_val != 0 {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            let mut last = LAST_DRAG_POS.lock().unwrap();
            let dx = pt.x - last.x;
            let dy = pt.y - last.y;

            if dx != 0 || dy != 0 {
                if DRAG_IS_GROUP.load(Ordering::SeqCst) {
                    let snapshot = ACTIVE_DRAG_SNAPSHOT.lock().unwrap();
                    let mut updates = Vec::with_capacity(snapshot.len());

                    if let Ok(mut hdwp) = BeginDeferWindowPos(snapshot.len() as i32) {
                        for &h_val in snapshot.iter() {
                            let h = HWND(h_val as *mut std::ffi::c_void);
                            let mut r = RECT::default();
                            if GetWindowRect(h, &mut r).is_ok() {
                                let (nx, ny) = (r.left + dx, r.top + dy);
                                let (nw, nh) = (r.right - r.left, r.bottom - r.top);

                                hdwp = DeferWindowPos(
                                    hdwp,
                                    h,
                                    None,
                                    nx,
                                    ny,
                                    0,
                                    0,
                                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                                )
                                .unwrap_or(hdwp);
                                updates.push((h_val, (nx, ny, nw, nh)));
                            }
                        }
                        let _ = EndDeferWindowPos(hdwp);
                    }
                    super::update_canvas();

                    if !updates.is_empty() {
                        let mut windows = MARKDOWN_WINDOWS.lock().unwrap();
                        for (key, rect) in updates {
                            if windows.contains_key(&key) {
                                windows.insert(key, rect);
                            }
                        }
                    }
                } else {
                    let target_hwnd = HWND(target_val as *mut std::ffi::c_void);
                    crate::overlay::result::trigger_drag_window(target_hwnd, dx, dy);
                }

                last.x = pt.x;
                last.y = pt.y;
            }
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn handle_button_up(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let target_val = ACTIVE_DRAG_TARGET.load(Ordering::SeqCst);
    if target_val != 0 {
        use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;

        ACTIVE_DRAG_TARGET.store(0, Ordering::SeqCst);
        let is_group = DRAG_IS_GROUP.swap(false, Ordering::SeqCst);
        let _ = ReleaseCapture();
        super::update_canvas();

        // PERSISTENCE: Save window geometry after manual drag
        let target_hwnd = HWND(target_val as *mut std::ffi::c_void);
        if is_group {
            let snapshot = ACTIVE_DRAG_SNAPSHOT.lock().unwrap();
            for &h_val in snapshot.iter() {
                crate::overlay::result::event_handler::save_window_geometry(
                    HWND(h_val as *mut std::ffi::c_void),
                    "CANVAS_DRAG_GROUP",
                );
            }
        } else {
            crate::overlay::result::event_handler::save_window_geometry(
                target_hwnd,
                "CANVAS_DRAG_SINGLE",
            );
        }

        // Click vs Drag Check
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        let start = START_DRAG_POS.lock().unwrap();
        let dist_sq = ((pt.x - start.x).pow(2) + (pt.y - start.y).pow(2)) as f64;

        if dist_sq.sqrt() < 5.0 {
            let is_right_click = msg == WM_RBUTTONUP;
            let is_middle_click = msg == WM_MBUTTONUP;
            let target_hwnd = HWND(target_val as *mut std::ffi::c_void);

            if is_right_click {
                let group = crate::overlay::result::state::get_window_group(target_hwnd);
                for (h, _) in group {
                    let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            } else if is_middle_click {
                crate::overlay::result::trigger_close_all();
            } else {
                let _ = PostMessageW(Some(target_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }

        // Force Immediate Cursor Update to JS
        let scale = get_dpi_scale();
        let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let rel_x = pt.x - v_x;
        let rel_y = pt.y - v_y;
        let logical_x = (rel_x as f64 / scale) as i32;
        let logical_y = (rel_y as f64 / scale) as i32;
        CANVAS_WEBVIEW.with(|cell| {
            if let Some(webview) = cell.borrow().as_ref() {
                let script = format!("window.updateCursorPosition({}, {});", logical_x, logical_y);
                let _ = webview.evaluate_script(&script);
            }
        });

        super::window::send_windows_update();

        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn handle_timer(wparam: WPARAM) {
    if wparam.0 == CURSOR_POLL_TIMER_ID {
        if ACTIVE_DRAG_TARGET.load(Ordering::SeqCst) == 0 {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() {
                let scale = get_dpi_scale();
                let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let rel_x = pt.x - v_x;
                let rel_y = pt.y - v_y;
                let logical_x = (rel_x as f64 / scale) as i32;
                let logical_y = (rel_y as f64 / scale) as i32;

                CANVAS_WEBVIEW.with(|cell| {
                    if let Some(webview) = cell.borrow().as_ref() {
                        let script =
                            format!("window.updateCursorPosition({}, {});", logical_x, logical_y);
                        let _ = webview.evaluate_script(&script);
                    }
                });
            }
        }
    }
}

unsafe fn handle_display_change(hwnd: HWND) {
    let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let v_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let v_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

    let _ = SetWindowPos(
        hwnd,
        None,
        v_x,
        v_y,
        v_w,
        v_h,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    CANVAS_WEBVIEW.with(|cell| {
        if let Some(webview) = cell.borrow().as_ref() {
            let _ = webview.set_bounds(Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    v_w as u32, v_h as u32,
                )),
            });
        }
    });
}

/// Send updated window data to the canvas
pub fn send_windows_update() {
    // Check if theme has changed and inject new CSS if needed
    let is_dark = crate::overlay::is_dark_mode();
    let last_dark = LAST_THEME_IS_DARK.load(Ordering::SeqCst);
    if is_dark != last_dark {
        let new_css = get_canvas_theme_css(is_dark);
        let content_escaped = new_css.replace('`', "\\`").replace('\\', "\\\\");
        let script = format!(
            "var s = document.getElementById('theme-css'); if(s) s.innerHTML = `{}`;",
            content_escaped
        );
        CANVAS_WEBVIEW.with(|cell| {
            if let Some(webview) = cell.borrow().as_ref() {
                let _ = webview.evaluate_script(&script);
            }
        });
        LAST_THEME_IS_DARK.store(is_dark, Ordering::SeqCst);
    }

    let windows_data = {
        let states = WINDOW_STATES.lock().unwrap();
        let windows = MARKDOWN_WINDOWS.lock().unwrap();

        let mut data = serde_json::Map::new();

        // Check for any active native interaction (Resizing/Moving)
        let any_interacting = states.values().any(|s| {
            matches!(
                s.interaction_mode,
                super::super::state::InteractionMode::Resizing(_)
                    | super::super::state::InteractionMode::ResizingGroup(_, _)
                    | super::super::state::InteractionMode::DraggingWindow
                    | super::super::state::InteractionMode::DraggingGroup(_)
            )
        });

        // If ANY drag (custom or native) or resize is active, hide ALL buttons
        let dragging_target = ACTIVE_DRAG_TARGET.load(Ordering::SeqCst);
        if dragging_target != 0 || any_interacting {
            let json = serde_json::to_string(&data).unwrap_or_default();
            CANVAS_WEBVIEW.with(|cell| {
                if let Some(webview) = cell.borrow().as_ref() {
                    let script = format!("window.updateWindows({});", json);
                    let _ = webview.evaluate_script(&script);
                }
            });
            return;
        }

        for (&hwnd_key, &(x, y, w, h)) in windows.iter() {
            let state = states.get(&hwnd_key);

            let state_obj = serde_json::json!({
                "copySuccess": state.map(|s| s.copy_success).unwrap_or(false),
                "hasUndo": state.map(|s| !s.text_history.is_empty()).unwrap_or(false),
                "hasRedo": state.map(|s| !s.redo_history.is_empty()).unwrap_or(false),
                "navDepth": state.map(|s| s.navigation_depth).unwrap_or(0),
                "maxNavDepth": state.map(|s| s.max_navigation_depth).unwrap_or(0),
                "ttsLoading": state.map(|s| s.tts_loading).unwrap_or(false),
                "ttsSpeaking": state.map(|s| s.tts_request_id != 0 && !s.tts_loading).unwrap_or(false),
                "isMarkdown": state.map(|s| s.is_markdown_mode).unwrap_or(false),
                "isBrowsing": state.map(|s| s.is_browsing).unwrap_or(false),
                "isEditing": state.map(|s| s.is_editing).unwrap_or(false),
                "inputText": state.map(|s| s.input_text.clone()).unwrap_or_default(),
                "opacityPercent": state.map(|s| s.opacity_percent).unwrap_or(100),
            });

            let scale = get_dpi_scale();
            let v_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
            let v_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };

            let rel_x = x - v_x;
            let rel_y = y - v_y;

            let logical_x = (rel_x as f64 / scale) as i32;
            let logical_y = (rel_y as f64 / scale) as i32;
            let logical_w = (w as f64 / scale) as i32;
            let logical_h = (h as f64 / scale) as i32;

            data.insert(
                hwnd_key.to_string(),
                serde_json::json!({
                    "rect": { "x": logical_x, "y": logical_y, "w": logical_w, "h": logical_h },
                    "state": state_obj
                }),
            );
        }

        serde_json::Value::Object(data)
    };

    CANVAS_WEBVIEW.with(|cell| {
        if let Some(webview) = cell.borrow().as_ref() {
            let script = format!("window.updateWindows({});", windows_data);
            let _ = webview.evaluate_script(&script);
        }
    });
}
