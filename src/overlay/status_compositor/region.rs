use super::protocol::{PhysicalRect, StatusSnapshot};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, HRGN, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    ShowWindow,
};

pub(super) fn initialize(hwnd: HWND) {
    unsafe {
        let empty = CreateRectRgn(0, 0, 0, 0);
        let _ = SetWindowRgn(hwnd, Some(empty), false);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub(super) fn update(hwnd: HWND, scene: &StatusSnapshot) {
    let display = super::display_metrics(hwnd);
    let visible = visible_rects(scene);
    unsafe {
        let combined = CreateRectRgn(0, 0, 0, 0);
        let mut bounded_count = 0;
        for rect in visible {
            let Some(rect) = clip_to_display(rect, display) else {
                continue;
            };
            bounded_count += 1;
            union_rect(
                combined,
                rect.x - display.x,
                rect.y - display.y,
                rect.width,
                rect.height,
            );
        }
        let _ = SetWindowRgn(hwnd, Some(combined), true);
        if bounded_count == 0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

fn visible_rects(scene: &StatusSnapshot) -> Vec<PhysicalRect> {
    let mut rects = Vec::with_capacity(3);
    if let Some(recording) = scene
        .recording
        .as_ref()
        .filter(|recording| recording.visible)
    {
        rects.push(recording.rect);
    }
    if scene.progress.is_some() || !scene.notifications.is_empty() {
        rects.push(scene.notification_rect);
    }
    if scene.selection.capture_visible
        && (scene.selection.text_visible || scene.selection.image_visible)
    {
        rects.push(scene.selection.rect);
    }
    rects
}

fn clip_to_display(rect: PhysicalRect, display: super::DisplayMetrics) -> Option<PhysicalRect> {
    let left = i64::from(rect.x).max(i64::from(display.x));
    let top = i64::from(rect.y).max(i64::from(display.y));
    let right = (i64::from(rect.x) + i64::from(rect.width.max(0)))
        .min(i64::from(display.x) + i64::from(display.width));
    let bottom = (i64::from(rect.y) + i64::from(rect.height.max(0)))
        .min(i64::from(display.y) + i64::from(display.height));
    (right > left && bottom > top).then_some(PhysicalRect {
        x: left as i32,
        y: top as i32,
        width: (right - left) as i32,
        height: (bottom - top) as i32,
    })
}

unsafe fn union_rect(region: HRGN, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        let rect = CreateRectRgn(x, y, x.saturating_add(width), y.saturating_add(height));
        let _ = CombineRgn(Some(region), Some(region), Some(rect), RGN_OR);
        let _ = DeleteObject(rect.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::status_compositor::protocol::{NotificationScene, SelectionScene};

    fn rect(x: i32, y: i32, width: i32, height: i32) -> PhysicalRect {
        PhysicalRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn empty_snapshot_never_exposes_the_full_desktop() {
        assert!(visible_rects(&StatusSnapshot::default()).is_empty());
    }

    #[test]
    fn selection_requires_content_and_capture_visibility() {
        let mut scene = StatusSnapshot {
            selection: SelectionScene {
                rect: rect(100, 200, 300, 80),
                text_visible: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(visible_rects(&scene).is_empty());
        scene.selection.capture_visible = true;
        assert_eq!(visible_rects(&scene), vec![rect(100, 200, 300, 80)]);
    }

    #[test]
    fn notifications_are_bounded_to_their_scene_rectangle() {
        let scene = StatusSnapshot {
            notification_rect: rect(-100, 40, 500, 160),
            notifications: vec![NotificationScene {
                id: 1,
                title: String::new(),
                snippet: String::new(),
                kind: "info".to_string(),
                duration_ms: None,
            }],
            ..Default::default()
        };
        assert_eq!(visible_rects(&scene), vec![rect(-100, 40, 500, 160)]);
    }

    #[test]
    fn clipping_handles_negative_virtual_desktop_origins() {
        let display = super::super::DisplayMetrics {
            x: -1920,
            y: -200,
            width: 3840,
            height: 1280,
            scale: 1.5,
        };
        assert_eq!(
            clip_to_display(rect(-2000, -250, 200, 200), display),
            Some(rect(-1920, -200, 120, 150))
        );
    }
}
