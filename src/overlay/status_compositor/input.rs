use super::protocol::PhysicalRect;
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecordingTarget {
    Pause,
    Cancel,
}

#[derive(Clone, Copy)]
struct ActiveDrag {
    start_cursor: POINT,
    start_rect: PhysicalRect,
}

static ACTIVE_DRAG: Mutex<Option<ActiveDrag>> = Mutex::new(None);
static BUTTON_REGIONS: Mutex<Option<(PhysicalRect, PhysicalRect)>> = Mutex::new(None);
static PRESSED_TARGET: Mutex<Option<RecordingTarget>> = Mutex::new(None);
static LAST_FEEDBACK: Mutex<(Option<RecordingTarget>, bool)> = Mutex::new((None, false));

pub(super) fn set_button_regions(pause: PhysicalRect, cancel: PhysicalRect) {
    *BUTTON_REGIONS.lock().unwrap() = Some((pause, cancel));
}

pub(super) fn target_at_cursor() -> Option<RecordingTarget> {
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return None;
    }
    target_for_point((*BUTTON_REGIONS.lock().unwrap())?, cursor)
}

fn target_for_point(
    (pause, cancel): (PhysicalRect, PhysicalRect),
    cursor: POINT,
) -> Option<RecordingTarget> {
    if contains(pause, cursor) {
        Some(RecordingTarget::Pause)
    } else if contains(cancel, cursor) {
        Some(RecordingTarget::Cancel)
    } else {
        None
    }
}

pub(super) fn begin_button(hwnd: HWND, target: RecordingTarget) {
    *PRESSED_TARGET.lock().unwrap() = Some(target);
    unsafe {
        let _ = SetCapture(hwnd);
    }
}

pub(super) fn finish_button() -> Option<RecordingTarget> {
    let pressed = PRESSED_TARGET.lock().unwrap().take();
    let activated = pressed.filter(|target| Some(*target) == target_at_cursor());
    unsafe {
        let _ = ReleaseCapture();
    }
    activated
}

pub(super) fn cancel_button() -> bool {
    if PRESSED_TARGET.lock().unwrap().take().is_none() {
        return false;
    }
    unsafe {
        let _ = ReleaseCapture();
    }
    true
}

pub(super) fn button_pressed() -> bool {
    PRESSED_TARGET.lock().unwrap().is_some()
}

pub(super) fn feedback_change() -> Option<(Option<RecordingTarget>, bool)> {
    let hovered = target_at_cursor();
    let active = PRESSED_TARGET
        .lock()
        .unwrap()
        .is_some_and(|target| Some(target) == hovered);
    let next = (hovered, active);
    let mut previous = LAST_FEEDBACK.lock().unwrap();
    if *previous == next {
        return None;
    }
    *previous = next;
    Some(next)
}

pub(super) fn begin(hwnd: HWND, rect: PhysicalRect) {
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    *ACTIVE_DRAG.lock().unwrap() = Some(ActiveDrag {
        start_cursor: cursor,
        start_rect: rect,
    });
    unsafe {
        let _ = SetCapture(hwnd);
    }
}

pub(super) fn update() -> Option<PhysicalRect> {
    let active = *ACTIVE_DRAG.lock().unwrap();
    let active = active?;
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return None;
    }
    Some(PhysicalRect {
        x: active
            .start_rect
            .x
            .saturating_add(cursor.x - active.start_cursor.x),
        y: active
            .start_rect
            .y
            .saturating_add(cursor.y - active.start_cursor.y),
        ..active.start_rect
    })
}

pub(super) fn finish() -> bool {
    if ACTIVE_DRAG.lock().unwrap().take().is_none() {
        return false;
    }
    unsafe {
        let _ = ReleaseCapture();
    }
    true
}

pub(super) fn active() -> bool {
    ACTIVE_DRAG.lock().unwrap().is_some()
}

fn contains(rect: PhysicalRect, point: POINT) -> bool {
    point.x >= rect.x
        && point.x < rect.x.saturating_add(rect.width)
        && point.y >= rect.y
        && point.y < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_button_regions_have_exclusive_edges() {
        let regions = (
            PhysicalRect {
                x: -40,
                y: 20,
                width: 34,
                height: 34,
            },
            PhysicalRect {
                x: 80,
                y: 20,
                width: 34,
                height: 34,
            },
        );
        assert_eq!(
            target_for_point(regions, POINT { x: -40, y: 20 }),
            Some(RecordingTarget::Pause)
        );
        assert_eq!(target_for_point(regions, POINT { x: -6, y: 20 }), None);
        assert_eq!(
            target_for_point(regions, POINT { x: 113, y: 53 }),
            Some(RecordingTarget::Cancel)
        );
        assert_eq!(target_for_point(regions, POINT { x: 114, y: 54 }), None);
    }
}
