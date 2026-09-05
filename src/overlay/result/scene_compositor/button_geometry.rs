use super::{ActiveResize, DragTarget, ResizeEdge};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER, SetWindowPos,
};

pub(super) unsafe fn place_targets(targets: &[DragTarget], dx: i32, dy: i32, live_only: bool) {
    unsafe {
        let selected = targets
            .iter()
            .filter(|target| target.native_backed && (!live_only || target.live_native))
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return;
        }
        let Ok(mut batch) = BeginDeferWindowPos(selected.len() as i32) else {
            return;
        };
        for target in selected {
            let hwnd = HWND(target.id as *mut std::ffi::c_void);
            let (x, y) = translated_origin(target.start_rect, dx, dy);
            batch = DeferWindowPos(
                batch,
                hwnd,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .unwrap_or(batch);
        }
        let _ = EndDeferWindowPos(batch);
    }
}

pub(crate) fn translated_origin(rect: RECT, dx: i32, dy: i32) -> (i32, i32) {
    (rect.left.saturating_add(dx), rect.top.saturating_add(dy))
}

impl ResizeEdge {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "n" => Self::new(true, false, false, false),
            "s" => Self::new(false, true, false, false),
            "e" => Self::new(false, false, true, false),
            "w" => Self::new(false, false, false, true),
            "ne" => Self::new(true, false, true, false),
            "nw" => Self::new(true, false, false, true),
            "se" => Self::new(false, true, true, false),
            "sw" => Self::new(false, true, false, true),
            _ => return None,
        })
    }

    const fn new(north: bool, south: bool, east: bool, west: bool) -> Self {
        Self {
            north,
            south,
            east,
            west,
        }
    }
}

pub(crate) fn resized_rect(mut rect: RECT, edge: ResizeEdge, dx: i32, dy: i32) -> RECT {
    if edge.west {
        rect.left = rect.left.saturating_add(dx).min(
            rect.right
                .saturating_sub(super::super::super::event_handler::MIN_WINDOW_WIDTH),
        );
    }
    if edge.east {
        rect.right = rect.right.saturating_add(dx).max(
            rect.left
                .saturating_add(super::super::super::event_handler::MIN_WINDOW_WIDTH),
        );
    }
    if edge.north {
        rect.top = rect.top.saturating_add(dy).min(
            rect.bottom
                .saturating_sub(super::super::super::event_handler::MIN_WINDOW_HEIGHT),
        );
    }
    if edge.south {
        rect.bottom = rect.bottom.saturating_add(dy).max(
            rect.top
                .saturating_add(super::super::super::event_handler::MIN_WINDOW_HEIGHT),
        );
    }
    rect
}

pub(super) unsafe fn place_resized_target(resize: &ActiveResize, dx: i32, dy: i32) {
    if !resize.native_backed {
        return;
    }
    unsafe {
        let hwnd = HWND(resize.id as *mut std::ffi::c_void);
        let rect = resized_rect(resize.start_rect, resize.edge, dx, dy);
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}
