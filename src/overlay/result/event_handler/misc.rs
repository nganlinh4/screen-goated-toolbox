use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::overlay::result::button_canvas;
use crate::overlay::result::markdown_view;
use crate::overlay::result::paint;
use crate::overlay::result::state::WINDOW_STATES;

pub const WM_CREATE_WEBVIEW: u32 = WM_USER + 200;
pub const WM_SHOW_MARKDOWN: u32 = WM_USER + 201;
pub const WM_HIDE_MARKDOWN: u32 = WM_USER + 202;
pub const WM_RESIZE_MARKDOWN: u32 = WM_USER + 203;
pub const WM_UNDO_CLICK: u32 = WM_USER + 210;
pub const WM_REDO_CLICK: u32 = WM_USER + 211;
pub const WM_COPY_CLICK: u32 = WM_USER + 212;
pub const WM_EDIT_CLICK: u32 = WM_USER + 213;
pub const WM_BACK_CLICK: u32 = WM_USER + 214;
pub const WM_FORWARD_CLICK: u32 = WM_USER + 215;
pub const WM_SPEAKER_CLICK: u32 = WM_USER + 216;
pub const WM_DOWNLOAD_CLICK: u32 = WM_USER + 217;

pub unsafe fn handle_erase_bkgnd(_hwnd: HWND, _wparam: WPARAM) -> LRESULT {
    LRESULT(1)
}

// handle_ctl_color_edit removed (was for native edit control)

pub unsafe fn handle_destroy(hwnd: HWND) -> LRESULT {
    // Clean up this window's resources only — callers are responsible for
    // deciding whether to close siblings (group close) or all windows.
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.remove(&(hwnd.0 as isize)) {
            // Signal cancellation token to stop this branch's processing
            if let Some(ref token) = state.cancellation_token {
                token.cancel();
            }

            // Stop TTS if speaking
            if state.tts_request_id != 0 {
                crate::api::tts::TTS_MANAGER.stop_if_active(state.tts_request_id);
            }

            // Cleanup GDI resources
            if !state.content_bitmap.is_invalid() {
                let _ = DeleteObject(state.content_bitmap.into());
            }
            if !state.bg_bitmap.is_invalid() {
                let _ = DeleteObject(state.bg_bitmap.into());
            }
        }
    }

    // Cleanup markdown webview and timer (outside lock)
    let _ = KillTimer(Some(hwnd), 2);
    markdown_view::destroy_markdown_webview(hwnd);

    // Unregister from button canvas (outside lock to prevent deadlock)
    button_canvas::unregister_markdown_window(hwnd);

    LRESULT(0)
}

pub unsafe fn handle_paint(hwnd: HWND) -> LRESULT {
    paint::paint_window(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_keydown() -> LRESULT {
    LRESULT(0)
}

pub unsafe fn handle_display_change(hwnd: HWND) -> LRESULT {
    // When monitor topology changes, check if window is still on-screen.
    // If not (e.g. secondary monitor removed), move it to primary monitor.
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_ok() {
        let center_x = (rect.left + rect.right) / 2;
        let center_y = (rect.top + rect.bottom) / 2;
        let center = POINT {
            x: center_x,
            y: center_y,
        };

        // Check if the center point maps to any monitor
        let h_monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONULL);

        if h_monitor.is_invalid() {
            // Window is off-screen. Move to Primary Monitor center.
            let h_primary = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO::default();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

            if GetMonitorInfoW(h_primary, &mut mi).as_bool() {
                let work = mi.rcWork;
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;

                // Center on primary monitor work area
                let new_x = work.left + (work.right - work.left - w) / 2;
                let new_y = work.top + (work.bottom - work.top - h) / 2;

                let _ = SetWindowPos(
                    hwnd,
                    None,
                    new_x,
                    new_y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );

                // IMPORTANT: Update button canvas about the new position
                button_canvas::update_window_position(hwnd);
            }
        }
    }
    LRESULT(0)
}

pub unsafe fn handle_create_webview(hwnd: HWND) -> LRESULT {
    // Get the text to render
    let (full_text, is_hovered) = {
        let states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&(hwnd.0 as isize)) {
            (state.full_text.clone(), state.is_hovered)
        } else {
            (String::new(), false)
        }
    };

    if markdown_view::has_markdown_webview(hwnd) {
        // WebView was pre-created, just show and update it
        markdown_view::update_markdown_content(hwnd, &full_text);
        markdown_view::show_markdown_webview(hwnd);
        markdown_view::resize_markdown_webview(hwnd, is_hovered);
        markdown_view::fit_font_to_window(hwnd);
        // Register with button canvas for floating buttons
        button_canvas::register_markdown_window(hwnd);
    } else {
        // Try to create WebView
        let result = markdown_view::create_markdown_webview(hwnd, &full_text, is_hovered);
        if !result {
            // Failed to create - revert markdown mode
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.is_markdown_mode = false;
            }
        } else {
            markdown_view::resize_markdown_webview(hwnd, is_hovered);
            markdown_view::fit_font_to_window(hwnd);
            // Register with button canvas for floating buttons
            button_canvas::register_markdown_window(hwnd);
        }
    }

    // IMPORTANT: If refine input is active, resize markdown to leave room for it
    // AND bring refine input to top so it stays visible
    // NOTE: Refine input is now part of button_canvas (overlay), so no resizing needed.

    let _ = InvalidateRect(Some(hwnd), None, false);
    LRESULT(0)
}

pub unsafe fn handle_show_markdown(hwnd: HWND) -> LRESULT {
    markdown_view::show_markdown_webview(hwnd);
    let _ = InvalidateRect(Some(hwnd), None, false);
    LRESULT(0)
}

pub unsafe fn handle_hide_markdown(hwnd: HWND) -> LRESULT {
    markdown_view::hide_markdown_webview(hwnd);
    let _ = InvalidateRect(Some(hwnd), None, false);
    LRESULT(0)
}

pub unsafe fn handle_resize_markdown(hwnd: HWND) -> LRESULT {
    let is_hovered = {
        let states = WINDOW_STATES.lock().unwrap();
        states
            .get(&(hwnd.0 as isize))
            .map(|s| s.is_hovered)
            .unwrap_or(false)
    };
    markdown_view::resize_markdown_webview(hwnd, is_hovered);
    markdown_view::fit_font_to_window(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_back_click(hwnd: HWND) -> LRESULT {
    markdown_view::go_back(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_forward_click(hwnd: HWND) -> LRESULT {
    markdown_view::go_forward(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_download_click(hwnd: HWND) -> LRESULT {
    let text = {
        let states = WINDOW_STATES.lock().unwrap();
        states
            .get(&(hwnd.0 as isize))
            .map(|s| s.full_text.clone())
            .unwrap_or_default()
    };
    if !text.is_empty() {
        markdown_view::save_html_file(&text);
    }
    LRESULT(0)
}
