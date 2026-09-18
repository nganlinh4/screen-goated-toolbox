//! Optional persistent-region editing mode of the ordinary selection window.
use super::state::*;
use std::cell::RefCell;
use windows::Win32::{
    Foundation::*,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub(super) struct Editor {
    pub scale: f32,
    pub rect: RECT,
    pub bounds: RECT,
    pub hover: Option<usize>,
    drag: Option<(usize, POINT, RECT)>,
    pressed: Option<usize>,
    saved: bool,
    pub icons: [(usize, Vec<u8>); 2],
}
thread_local! { static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) }; }

pub(super) fn begin(mut rect: RECT, bounds: RECT) {
    use crate::gui::icons::{Icon, native_mask};
    use windows::Win32::{
        Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromRect},
        UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
    };
    let mut dpi_x = 96;
    let mut dpi_y = 96;
    unsafe {
        let _ = GetDpiForMonitor(
            MonitorFromRect(&bounds, MONITOR_DEFAULTTONEAREST),
            MDT_EFFECTIVE_DPI,
            &mut dpi_x,
            &mut dpi_y,
        );
    }
    let scale = (dpi_x as f32 / 96.0).clamp(1.0, 4.0);
    rect.left = rect.left.clamp(bounds.left, bounds.right - 16);
    rect.top = rect.top.clamp(bounds.top, bounds.bottom - 16);
    rect.right = rect.right.clamp(rect.left + 16, bounds.right);
    rect.bottom = rect.bottom.clamp(rect.top + 16, bounds.bottom);
    let icon_size = (24.0 * scale).round() as u32;
    EDITOR.with_borrow_mut(|state| {
        *state = Some(Editor {
            scale,
            rect,
            bounds,
            hover: None,
            drag: None,
            pressed: None,
            saved: false,
            icons: [
                native_mask(Icon::Close, icon_size),
                native_mask(Icon::Check, icon_size),
            ],
        })
    });
}
pub(super) fn finish() -> Option<RECT> {
    EDITOR.with_borrow_mut(|state| {
        state
            .take()
            .filter(|state| state.saved)
            .map(|state| state.rect)
    })
}
pub(super) fn active() -> bool {
    EDITOR.with_borrow(|s| s.is_some())
}
pub(super) fn rectangle() -> Option<RECT> {
    EDITOR.with_borrow(|s| s.as_ref().map(|s| s.rect))
}
pub(super) fn with_editor(f: impl FnOnce(&Editor)) {
    EDITOR.with_borrow(|s| {
        if let Some(s) = s {
            f(s)
        }
    });
}

pub(super) fn controls(state: &Editor) -> [RECT; 2] {
    let b = state.bounds;
    let px = |v: f32| (v * state.scale).round() as i32;
    let y = b.bottom - px(72.0);
    [
        RECT {
            left: b.right - px(136.0),
            top: y,
            right: b.right - px(88.0),
            bottom: y + px(48.0),
        },
        RECT {
            left: b.right - px(80.0),
            top: y,
            right: b.right - px(32.0),
            bottom: y + px(48.0),
        },
    ]
}
pub(super) fn handles(rect: RECT, bounds: RECT, inset: i32) -> [POINT; 8] {
    let x = (rect.left + rect.right) / 2;
    let y = (rect.top + rect.bottom) / 2;
    [
        (rect.left, rect.top),
        (x, rect.top),
        (rect.right, rect.top),
        (rect.right, y),
        (rect.right, rect.bottom),
        (x, rect.bottom),
        (rect.left, rect.bottom),
        (rect.left, y),
    ]
    .map(|(x, y)| POINT {
        x: x.clamp(bounds.left + inset, bounds.right - inset),
        y: y.clamp(bounds.top + inset, bounds.bottom - inset),
    })
}
fn contains(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}
fn hit(s: &Editor, p: POINT) -> Option<usize> {
    if let Some(i) = controls(s).iter().position(|r| contains(*r, p)) {
        return Some(9 + i);
    }
    let radius = (10.0 * s.scale).round() as i32;
    if let Some(i) = handles(s.rect, s.bounds, (6.0 * s.scale).round() as i32)
        .iter()
        .position(|q| (p.x - q.x).abs() <= radius && (p.y - q.y).abs() <= radius)
    {
        return Some(i);
    }
    contains(s.rect, p).then_some(8)
}

fn dragged(edge: usize, start: RECT, delta: POINT, b: RECT) -> RECT {
    let mut r = start;
    let min_w = 16.min(b.right - b.left);
    let min_h = 16.min(b.bottom - b.top);
    if edge == 8 {
        let dx = delta.x.clamp(b.left - r.left, b.right - r.right);
        let dy = delta.y.clamp(b.top - r.top, b.bottom - r.bottom);
        r.left += dx;
        r.right += dx;
        r.top += dy;
        r.bottom += dy;
    } else {
        if [0, 6, 7].contains(&edge) {
            r.left = (start.left + delta.x).clamp(b.left, start.right - min_w);
        }
        if [2, 3, 4].contains(&edge) {
            r.right = (start.right + delta.x).clamp(start.left + min_w, b.right);
        }
        if [0, 1, 2].contains(&edge) {
            r.top = (start.top + delta.y).clamp(b.top, start.bottom - min_h);
        }
        if [4, 5, 6].contains(&edge) {
            r.bottom = (start.bottom + delta.y).clamp(start.top + min_h, b.bottom);
        }
    }
    r
}

pub(super) unsafe fn message(hwnd: HWND, msg: u32) -> Option<LRESULT> {
    if !active() {
        return None;
    }
    unsafe {
        if msg == WM_DISPLAYCHANGE {
            EDITOR.with_borrow_mut(|s| {
                if let Some(s) = s {
                    s.saved = false;
                }
            });
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            return Some(LRESULT(0));
        }
        if ![
            WM_LBUTTONDOWN,
            WM_LBUTTONUP,
            WM_MOUSEMOVE,
            WM_SETCURSOR,
            WM_MOUSEWHEEL,
            WM_RBUTTONDOWN,
            WM_RBUTTONUP,
            WM_CAPTURECHANGED,
        ]
        .contains(&msg)
        {
            return None;
        }
        if IS_FADING_OUT {
            return Some(LRESULT(0));
        }
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let mut close = false;
        let mut repaint = false;
        let mut capture = false;
        let mut release = false;
        EDITOR.with_borrow_mut(|state| {
            let s = state.as_mut().unwrap();
            let over = hit(s, p);
            if msg == WM_SETCURSOR {
                let cursor = match s.drag.map(|d| d.0).or(over) {
                    Some(0 | 4) => IDC_SIZENWSE,
                    Some(2 | 6) => IDC_SIZENESW,
                    Some(1 | 5) => IDC_SIZENS,
                    Some(3 | 7) => IDC_SIZEWE,
                    Some(8) => IDC_SIZEALL,
                    Some(9 | 10) => IDC_HAND,
                    _ => IDC_ARROW,
                };
                SetCursor(LoadCursorW(None, cursor).ok());
            } else if msg == WM_LBUTTONDOWN {
                s.pressed = over;
                capture = true;
                if let Some(edge) = over.filter(|e| *e <= 8) {
                    s.drag = Some((edge, p, s.rect));
                }
            } else if msg == WM_LBUTTONUP {
                if s.pressed == over
                    && let Some(button) = over.filter(|b| *b >= 9)
                {
                    s.saved = button == 10;
                    close = true;
                }
                s.pressed = None;
                s.drag = None;
                release = true;
                repaint = true;
            } else if msg == WM_CAPTURECHANGED {
                s.drag = None;
                s.pressed = None;
            } else if msg == WM_MOUSEMOVE {
                repaint = s.hover != over;
                s.hover = over;
                if let Some((edge, origin, start)) = s.drag {
                    s.rect = dragged(
                        edge,
                        start,
                        POINT {
                            x: p.x - origin.x,
                            y: p.y - origin.y,
                        },
                        s.bounds,
                    );
                    repaint = true;
                }
            }
            START_POS = POINT {
                x: s.rect.left,
                y: s.rect.top,
            };
            CURR_POS = POINT {
                x: s.rect.right,
                y: s.rect.bottom,
            };
        });
        if capture {
            SetCapture(hwnd);
        }
        if release {
            let _ = ReleaseCapture();
        }
        if close {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if repaint {
            super::render::sync_layered_window_contents(hwnd);
        }
        Some(LRESULT(if msg == WM_SETCURSOR { 1 } else { 0 }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_save_returns_geometry_and_editor_state_is_released() {
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        begin(bounds, bounds);
        assert!(active());
        assert!(finish().is_none());
        assert!(!active());
        begin(bounds, bounds);
        EDITOR.with_borrow_mut(|state| state.as_mut().unwrap().saved = true);
        assert_eq!(finish(), Some(bounds));
        assert!(!active());
    }

    #[test]
    fn tiny_saved_regions_expand_to_safe_editable_bounds() {
        let bounds = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        begin(
            RECT {
                left: -1,
                top: 1079,
                right: 0,
                bottom: 1080,
            },
            bounds,
        );
        let r = rectangle().unwrap();
        assert_eq!((r.right - r.left, r.bottom - r.top), (16, 16));
        assert_eq!((r.right, r.bottom), (0, 1080));
        finish();
    }
    #[test]
    fn all_handles_clamp_without_flipping_and_movement_preserves_size() {
        let bounds = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let r = RECT {
            left: -1700,
            top: 100,
            right: -500,
            bottom: 800,
        };
        for edge in 0..9 {
            for d in [-10000, 10000] {
                let next = dragged(edge, r, POINT { x: d, y: d }, bounds);
                assert!(
                    next.left >= bounds.left
                        && next.top >= bounds.top
                        && next.right <= bounds.right
                        && next.bottom <= bounds.bottom
                );
                assert!(next.right - next.left >= 16 && next.bottom - next.top >= 16);
                if edge == 8 {
                    assert_eq!(
                        (next.right - next.left, next.bottom - next.top),
                        (1200, 700)
                    );
                }
            }
        }
        assert!(
            handles(bounds, bounds, 6)
                .iter()
                .all(|p| contains(bounds, *p))
        );
    }
}
