use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

use crate::overlay::result::layout::{
    get_copy_btn_rect, get_download_btn_rect, get_edit_btn_rect, get_markdown_btn_rect,
    get_resize_edge, get_speaker_btn_rect, get_undo_btn_rect, should_show_buttons,
};
use crate::overlay::result::state::{ResizeEdge, WINDOW_STATES};

pub unsafe fn handle_set_cursor(hwnd: HWND) -> LRESULT {
    unsafe {
        let mut cursor_id = PCWSTR(std::ptr::null());
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = ScreenToClient(hwnd, &mut pt);

        let is_over_edit = false;
        let mut is_streaming_active = false;
        {
            let states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get(&(hwnd.0 as isize)) {
                is_streaming_active = state.is_streaming_active;
            }
        }

        if is_over_edit {
            SetCursor(Some(LoadCursorW(None, IDC_IBEAM).unwrap()));
            return LRESULT(1);
        }

        let edge = get_resize_edge(rect.right, rect.bottom, pt.x, pt.y);

        match edge {
            ResizeEdge::Top | ResizeEdge::Bottom => cursor_id = IDC_SIZENS,
            ResizeEdge::Left | ResizeEdge::Right => cursor_id = IDC_SIZEWE,
            ResizeEdge::TopLeft | ResizeEdge::BottomRight => cursor_id = IDC_SIZENWSE,
            ResizeEdge::TopRight | ResizeEdge::BottomLeft => cursor_id = IDC_SIZENESW,
            ResizeEdge::None => {
                // Only show hand cursor on buttons if overlay is large enough AND not streaming
                if !is_streaming_active && should_show_buttons(rect.right, rect.bottom) {
                    let copy_rect = get_copy_btn_rect(rect.right, rect.bottom);
                    let edit_rect = get_edit_btn_rect(rect.right, rect.bottom);
                    let undo_rect = get_undo_btn_rect(rect.right, rect.bottom);

                    let on_copy = pt.x >= copy_rect.left
                        && pt.x <= copy_rect.right
                        && pt.y >= copy_rect.top
                        && pt.y <= copy_rect.bottom;
                    let on_edit = pt.x >= edit_rect.left
                        && pt.x <= edit_rect.right
                        && pt.y >= edit_rect.top
                        && pt.y <= edit_rect.bottom;

                    let mut has_history = false;
                    let mut is_browsing = false;
                    {
                        let states = WINDOW_STATES.lock().unwrap();
                        if let Some(state) = states.get(&(hwnd.0 as isize)) {
                            has_history = !state.text_history.is_empty();
                            is_browsing = state.is_browsing;
                        }
                    }

                    // Manual calc for Back button rect
                    let btn_size = 28;
                    let margin = 12;
                    let threshold_h = btn_size + (margin * 2);
                    let cy = if rect.bottom < threshold_h {
                        (rect.bottom as f32) / 2.0
                    } else {
                        (rect.bottom - margin - btn_size / 2) as f32
                    };
                    let cx_back = margin + btn_size / 2;
                    let cy_back = cy as i32;
                    let back_rect = RECT {
                        left: cx_back - 14,
                        top: cy_back - 14,
                        right: cx_back + 14,
                        bottom: cy_back + 14,
                    };

                    let on_back = is_browsing
                        && pt.x >= back_rect.left
                        && pt.x <= back_rect.right
                        && pt.y >= back_rect.top
                        && pt.y <= back_rect.bottom;

                    let on_undo = has_history
                        && pt.x >= undo_rect.left
                        && pt.x <= undo_rect.right
                        && pt.y >= undo_rect.top
                        && pt.y <= undo_rect.bottom;

                    let md_rect = get_markdown_btn_rect(rect.right, rect.bottom);
                    let on_md = pt.x >= md_rect.left
                        && pt.x <= md_rect.right
                        && pt.y >= md_rect.top
                        && pt.y <= md_rect.bottom;

                    let dl_rect = get_download_btn_rect(rect.right, rect.bottom);
                    let on_dl = pt.x >= dl_rect.left
                        && pt.x <= dl_rect.right
                        && pt.y >= dl_rect.top
                        && pt.y <= dl_rect.bottom;

                    let speaker_rect = get_speaker_btn_rect(rect.right, rect.bottom);
                    let on_speaker = pt.x >= speaker_rect.left
                        && pt.x <= speaker_rect.right
                        && pt.y >= speaker_rect.top
                        && pt.y <= speaker_rect.bottom;

                    if on_copy || on_edit || on_undo || on_md || on_back || on_dl || on_speaker {
                        cursor_id = IDC_HAND;
                    }
                }
            }
        }

        if !cursor_id.0.is_null() {
            SetCursor(Some(LoadCursorW(None, cursor_id).unwrap()));
            LRESULT(1)
        } else {
            // Hide system cursor when over window body to let procedural broom be the visual
            SetCursor(None);
            LRESULT(1)
        }
    }
}
