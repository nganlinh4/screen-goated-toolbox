use windows::Win32::Foundation::RECT;
use super::state::ResizeEdge;

pub fn get_copy_btn_rect(window_w: i32, window_h: i32) -> RECT {
    let btn_size = 28;
    let margin = 12;
    let threshold_h = btn_size + (margin * 2);
    let top = if window_h < threshold_h {
        (window_h - btn_size) / 2
    } else {
        window_h - margin - btn_size
    };

    RECT {
        left: window_w - margin - btn_size,
        top,
        right: window_w - margin,
        bottom: top + btn_size,
    }
}

pub fn get_edit_btn_rect(window_w: i32, window_h: i32) -> RECT {
    let copy_rect = get_copy_btn_rect(window_w, window_h);
    let gap = 8;
    let width = copy_rect.right - copy_rect.left;
    RECT {
        left: copy_rect.left - width - gap,
        top: copy_rect.top,
        right: copy_rect.left - gap,
        bottom: copy_rect.bottom
    }
}

pub fn get_undo_btn_rect(window_w: i32, window_h: i32) -> RECT {
    let edit_rect = get_edit_btn_rect(window_w, window_h);
    let gap = 8;
    let width = edit_rect.right - edit_rect.left;
    RECT {
        left: edit_rect.left - width - gap,
        top: edit_rect.top,
        right: edit_rect.left - gap,
        bottom: edit_rect.bottom
    }
}

pub fn get_resize_edge(width: i32, height: i32, x: i32, y: i32) -> ResizeEdge {
    let margin = 8;
    let left = x < margin;
    let right = x >= width - margin;
    let top = y < margin;
    let bottom = y >= height - margin;

    if top && left { ResizeEdge::TopLeft }
    else if top && right { ResizeEdge::TopRight }
    else if bottom && left { ResizeEdge::BottomLeft }
    else if bottom && right { ResizeEdge::BottomRight }
    else if left { ResizeEdge::Left }
    else if right { ResizeEdge::Right }
    else if top { ResizeEdge::Top }
    else if bottom { ResizeEdge::Bottom }
    else { ResizeEdge::None }
}
