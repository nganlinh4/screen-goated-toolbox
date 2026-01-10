use std::sync::Arc;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::overlay::result::button_canvas;
use crate::overlay::result::markdown_view;
use crate::overlay::result::paint;
use crate::overlay::result::refine_input;
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

pub unsafe fn handle_ctl_color_edit(wparam: WPARAM) -> LRESULT {
    let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
    SetBkMode(hdc, OPAQUE);
    SetBkColor(hdc, COLORREF(0x00FFFFFF));
    SetTextColor(hdc, COLORREF(0x00000000));
    let hbrush = GetStockObject(WHITE_BRUSH);
    LRESULT(hbrush.0 as isize)
}

pub unsafe fn handle_destroy(hwnd: HWND) -> LRESULT {
    // Collect windows to close (those sharing the same cancellation token)
    let windows_to_close: Vec<HWND>;
    let token_to_signal: Option<Arc<std::sync::atomic::AtomicBool>>;

    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.remove(&(hwnd.0 as isize)) {
            // Stop TTS if speaking
            if state.tts_request_id != 0 {
                crate::api::tts::TTS_MANAGER.stop_if_active(state.tts_request_id);
            }

            // Get the cancellation token from this window
            token_to_signal = state.cancellation_token.clone();

            // Find all other windows with the same cancellation token
            if let Some(ref token) = token_to_signal {
                // Signal cancellation first
                token.store(true, std::sync::atomic::Ordering::Relaxed);

                // Collect windows to close (can't close while iterating with lock held)
                windows_to_close = states
                    .iter()
                    .filter(|(_, s)| {
                        if let Some(ref other_token) = s.cancellation_token {
                            Arc::ptr_eq(token, other_token)
                        } else {
                            false
                        }
                    })
                    .map(|(k, _)| HWND(*k as *mut core::ffi::c_void))
                    .collect();
            } else {
                windows_to_close = Vec::new();
            }

            // Cleanup this window's resources
            if !state.content_bitmap.is_invalid() {
                let _ = DeleteObject(state.content_bitmap.into());
            }
            if !state.bg_bitmap.is_invalid() {
                let _ = DeleteObject(state.bg_bitmap.into());
            }
            if !state.edit_font.is_invalid() {
                let _ = DeleteObject(state.edit_font.into());
            }

            // Cleanup refine input if active (must be inside lock to check/set state if needed,
            // but hide_refine_input handles its own visibility)
            refine_input::hide_refine_input(hwnd);
        } else {
            windows_to_close = Vec::new();
        }
    }

    // Cleanup markdown webview and timer (outside lock)
    let _ = KillTimer(Some(hwnd), 2);
    markdown_view::destroy_markdown_webview(hwnd);

    // Unregister from button canvas (outside lock to prevent deadlock)
    button_canvas::unregister_markdown_window(hwnd);

    // Close all other windows in the same chain (after dropping the lock)
    for other_hwnd in windows_to_close {
        if other_hwnd != hwnd {
            let _ = PostMessageW(Some(other_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    LRESULT(0)
}

pub unsafe fn handle_paint(hwnd: HWND) -> LRESULT {
    paint::paint_window(hwnd);
    LRESULT(0)
}

pub unsafe fn handle_keydown() -> LRESULT {
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
    if refine_input::is_refine_input_active(hwnd) {
        // Resize markdown webview to account for refine input at top
        markdown_view::resize_markdown_webview(hwnd, is_hovered);
        // Bring refine input to top
        refine_input::bring_to_top(hwnd);
    }

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
