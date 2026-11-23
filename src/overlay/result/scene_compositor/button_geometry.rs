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
        let moved = defer_positions(
            &selected,
            |count| BeginDeferWindowPos(count as i32).ok(),
            |batch, target| {
                let hwnd = HWND(target.id as *mut std::ffi::c_void);
                let (x, y) = translated_origin(target.start_rect, dx, dy);
                DeferWindowPos(
                    batch,
                    hwnd,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .ok()
            },
            |batch| EndDeferWindowPos(batch).is_ok(),
        );
        if !moved {
            for target in selected {
                let hwnd = HWND(target.id as *mut std::ffi::c_void);
                let (x, y) = translated_origin(target.start_rect, dx, dy);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

fn defer_positions<T, B>(
    targets: &[T],
    begin: impl FnOnce(usize) -> Option<B>,
    mut defer: impl FnMut(B, &T) -> Option<B>,
    finish: impl FnOnce(B) -> bool,
) -> bool {
    let Some(mut batch) = begin(targets.len()) else {
        return false;
    };
    for target in targets {
        let Some(next) = defer(batch, target) else {
            return false;
        };
        batch = next;
    }
    finish(batch)
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

#[cfg(test)]
mod tests {
    use super::{DragTarget, defer_positions, place_targets};
    use std::cell::Cell;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetWindowRect, WS_EX_NOACTIVATE, WS_POPUP,
    };
    use windows::core::w;

    fn offscreen_window(left: i32) -> HWND {
        unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("Static"),
                w!(""),
                WS_POPUP,
                left,
                -20000,
                40,
                40,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        }
    }

    fn target(hwnd: HWND, left: i32) -> DragTarget {
        DragTarget {
            id: hwnd.0 as isize,
            start_rect: RECT {
                left,
                top: -20000,
                right: left + 40,
                bottom: -19960,
            },
            live_native: true,
            native_backed: true,
        }
    }

    #[test]
    fn a_member_destroyed_mid_drag_does_not_strand_the_rest_of_the_group() {
        let lefts = [-20000, -19900, -19800];
        let windows = lefts.map(offscreen_window);
        let targets = [0, 1, 2].map(|index| target(windows[index], lefts[index]));
        unsafe {
            DestroyWindow(windows[0]).unwrap();
            place_targets(&targets, 7, 11, false);
            for index in [1, 2] {
                let mut rect = RECT::default();
                GetWindowRect(windows[index], &mut rect).unwrap();
                assert_eq!((rect.left, rect.top), (lefts[index] + 7, -20000 + 11));
                DestroyWindow(windows[index]).unwrap();
            }
        }
    }

    #[test]
    fn failed_batch_is_abandoned_and_reported_for_fallback() {
        let finished = Cell::new(false);
        let attempted = Cell::new(0);
        let success = defer_positions(
            &[1, 2, 3],
            |_| Some(0),
            |batch, target| {
                attempted.set(attempted.get() + 1);
                (*target != 2).then_some(batch + 1)
            },
            |_| {
                finished.set(true);
                true
            },
        );
        assert!(!success);
        assert_eq!(attempted.get(), 2);
        assert!(!finished.get());

        assert!(!defer_positions::<_, ()>(
            &[1],
            |_| None,
            |_, _| panic!("no batch was created"),
            |_| panic!("no batch was created"),
        ));
        assert!(!defer_positions(
            &[1],
            |_| Some(()),
            |(), _| Some(()),
            |()| false
        ));
    }
}
