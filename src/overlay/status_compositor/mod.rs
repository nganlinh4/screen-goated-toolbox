mod child;
mod dcomp;
mod html;
mod input;
mod mailbox;
mod parent;
pub(crate) mod protocol;
mod region;
pub(crate) mod smoke;

use protocol::{HostCommand, NotificationScene, PhysicalRect, ProgressScene, RecordingScene};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

pub(crate) const CHILD_FLAG: &str = "--internal-status-compositor";
static NEXT_SCENE_ORDER: AtomicU64 = AtomicU64::new(1);
static NEXT_CAPTURE_REQUEST: AtomicU64 = AtomicU64::new(1);
static NEXT_PROGRESS_REMOVAL_REQUEST: AtomicU64 = AtomicU64::new(1);
const MAX_ACTIVE_NOTIFICATIONS: usize = 32;
const MAX_TITLE_CHARS: usize = 256;
const MAX_SNIPPET_CHARS: usize = 2_048;
const MAX_BADGE_TEXT_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DisplayMetrics {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

pub(crate) fn is_child_process() -> bool {
    std::env::args().any(|argument| argument == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    child::run()
}

pub(crate) fn shutdown_for_exit() {
    parent::shutdown_for_exit();
}

pub(crate) fn update_theme(is_dark: bool) {
    parent::SNAPSHOT.lock().unwrap().is_dark = is_dark;
    parent::send_if_running(HostCommand::Theme { is_dark });
}

pub(crate) fn recording_prepare(rect: PhysicalRect) {
    let scene = RecordingScene {
        rect,
        visible: false,
        state: "warmup".to_string(),
        rms: 0.0,
    };
    parent::SNAPSHOT.lock().unwrap().recording = Some(scene.clone());
    parent::send(HostCommand::RecordingPrepare { scene });
}

pub(crate) fn recording_show(rect: PhysicalRect) {
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    let scene = snapshot.recording.get_or_insert_with(|| RecordingScene {
        rect,
        visible: true,
        state: "warmup".to_string(),
        rms: 0.0,
    });
    scene.rect = rect;
    scene.visible = true;
    drop(snapshot);
    parent::send(HostCommand::RecordingShow { rect });
}

pub(crate) fn recording_update(state: &str, rms: f32) {
    if let Some(recording) = parent::SNAPSHOT.lock().unwrap().recording.as_mut() {
        recording.state = state.to_string();
        recording.rms = rms;
    }
    parent::send(HostCommand::RecordingUpdate {
        state: state.to_string(),
        rms,
    });
}

pub(crate) fn recording_hide() {
    if let Some(recording) = parent::SNAPSHOT.lock().unwrap().recording.as_mut() {
        recording.visible = false;
    }
    parent::send(HostCommand::RecordingHide);
}

pub(crate) fn add_notification(
    title: String,
    snippet: String,
    kind: &str,
    duration_ms: Option<u32>,
) {
    let notification = NotificationScene {
        id: NEXT_SCENE_ORDER.fetch_add(1, Ordering::SeqCst),
        title: bounded_text(title, MAX_TITLE_CHARS),
        snippet: bounded_text(snippet, MAX_SNIPPET_CHARS),
        kind: kind.to_string(),
        duration_ms,
    };
    let rect = notification_rect();
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    snapshot.notification_rect = rect;
    snapshot.notifications.push(notification.clone());
    let overflow = trim_notifications(&mut snapshot.notifications);
    let command = if overflow {
        HostCommand::Snapshot {
            scene: snapshot.clone(),
        }
    } else {
        HostCommand::NotificationAdd { rect, notification }
    };
    drop(snapshot);
    parent::send(command);
}

pub(crate) fn progress_upsert(title: String, snippet: String, progress: f32) {
    let rect = notification_rect();
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    let progress = ProgressScene {
        order: snapshot
            .progress
            .as_ref()
            .map(|progress| progress.order)
            .unwrap_or_else(|| NEXT_SCENE_ORDER.fetch_add(1, Ordering::SeqCst)),
        title: bounded_text(title, MAX_TITLE_CHARS),
        snippet: bounded_text(snippet, MAX_SNIPPET_CHARS),
        progress: progress.clamp(0.0, 100.0),
    };
    snapshot.notification_rect = rect;
    snapshot.progress = Some(progress.clone());
    drop(snapshot);
    parent::send(HostCommand::ProgressUpsert { rect, progress });
}

pub(crate) fn progress_remove() {
    parent::SNAPSHOT.lock().unwrap().progress = None;
    parent::send(HostCommand::ProgressRemove);
}

pub(crate) fn progress_remove_before_capture(timeout: Duration) -> bool {
    parent::SNAPSHOT.lock().unwrap().progress = None;
    let request_id = NEXT_PROGRESS_REMOVAL_REQUEST.fetch_add(1, Ordering::SeqCst);
    parent::send(HostCommand::ProgressRemoveBeforeCapture { request_id });
    parent::wait_for_progress_removal(request_id, timeout)
}

pub(crate) fn selection_show(rect: PhysicalRect, text: String) {
    let text = bounded_text(text, MAX_BADGE_TEXT_CHARS);
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    snapshot.selection.rect = rect;
    snapshot.selection.text_visible = true;
    snapshot.selection.selecting = false;
    snapshot.selection.text.clone_from(&text);
    drop(snapshot);
    parent::send(HostCommand::SelectionShow { rect, text });
}

pub(crate) fn selection_hide() {
    parent::SNAPSHOT.lock().unwrap().selection.text_visible = false;
    parent::send(HostCommand::SelectionHide);
}

pub(crate) fn selection_update(selecting: bool, text: String) {
    let text = bounded_text(text, MAX_BADGE_TEXT_CHARS);
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    snapshot.selection.selecting = selecting;
    snapshot.selection.text.clone_from(&text);
    drop(snapshot);
    parent::send(HostCommand::SelectionUpdate { selecting, text });
}

pub(crate) fn selection_position(rect: PhysicalRect) {
    parent::SNAPSHOT.lock().unwrap().selection.rect = rect;
    parent::send(HostCommand::SelectionPosition { rect });
}

pub(crate) fn image_badge_show(rect: PhysicalRect, text: String) {
    let text = bounded_text(text, MAX_BADGE_TEXT_CHARS);
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    snapshot.selection.rect = rect;
    snapshot.selection.image_visible = true;
    snapshot.selection.image_text.clone_from(&text);
    drop(snapshot);
    parent::send(HostCommand::ImageBadgeShow { rect, text });
}

pub(crate) fn image_badge_hide() {
    parent::SNAPSHOT.lock().unwrap().selection.image_visible = false;
    parent::send(HostCommand::ImageBadgeHide);
}

pub(crate) fn set_selection_capture_visible(visible: bool) -> bool {
    parent::SNAPSHOT.lock().unwrap().selection.capture_visible = visible;
    let request_id = NEXT_CAPTURE_REQUEST.fetch_add(1, Ordering::SeqCst);
    parent::send(HostCommand::SelectionCapture {
        visible,
        request_id,
    });
    parent::wait_for_capture(request_id, Duration::from_millis(100))
}

pub(crate) fn physical_rect(x: i32, y: i32, width: i32, height: i32) -> PhysicalRect {
    PhysicalRect {
        x,
        y,
        width,
        height,
    }
}

fn notification_rect() -> PhysicalRect {
    const WIDTH: i32 = 1200;
    const HEIGHT: i32 = 400;
    let screen_width =
        unsafe { GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN) };
    let screen_height =
        unsafe { GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN) };
    physical_rect(
        (screen_width - WIDTH) / 2,
        screen_height - HEIGHT - 100,
        WIDTH,
        HEIGHT,
    )
}

pub(super) fn display_metrics(hwnd: HWND) -> DisplayMetrics {
    let (x, y, width, height) = virtual_screen();
    DisplayMetrics {
        x,
        y,
        width,
        height,
        scale: (unsafe { GetDpiForWindow(hwnd) } as f64 / 96.0).max(1.0),
    }
}

pub(super) fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    }
}

pub(super) fn fit_rect_to_display(rect: PhysicalRect, display: DisplayMetrics) -> PhysicalRect {
    let width = rect.width.max(1).min(display.width);
    let height = rect.height.max(1).min(display.height);
    PhysicalRect {
        x: rect.x.clamp(display.x, display.x + display.width - width),
        y: rect.y.clamp(display.y, display.y + display.height - height),
        width,
        height,
    }
}

fn bounded_text(mut text: String, max_chars: usize) -> String {
    if let Some((byte_index, _)) = text.char_indices().nth(max_chars) {
        text.truncate(byte_index);
    }
    text
}

fn trim_notifications(notifications: &mut Vec<NotificationScene>) -> bool {
    if notifications.len() <= MAX_ACTIVE_NOTIFICATIONS {
        return false;
    }
    let remove = notifications.len() - MAX_ACTIVE_NOTIFICATIONS;
    notifications.drain(..remove);
    true
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn topology_reconciliation_handles_negative_origins_and_removed_monitors() {
        let display = DisplayMetrics {
            x: -1920,
            y: -200,
            width: 3840,
            height: 1280,
            scale: 1.5,
        };
        assert_eq!(
            fit_rect_to_display(physical_rect(-2500, 1400, 450, 70), display),
            physical_rect(-1920, 1010, 450, 70)
        );
        assert_eq!(
            fit_rect_to_display(physical_rect(1700, -300, 450, 70), display),
            physical_rect(1470, -200, 450, 70)
        );
    }

    #[test]
    fn status_payload_bounds_preserve_unicode_scalar_boundaries() {
        let value = bounded_text("가나다라마바사".to_string(), 4);
        assert_eq!(value, "가나다라");
        assert_eq!(value.chars().count(), 4);
    }

    #[test]
    fn notification_snapshot_keeps_only_the_newest_bounded_stack() {
        let mut notifications: Vec<_> = (0..MAX_ACTIVE_NOTIFICATIONS as u64 + 5)
            .map(|id| NotificationScene {
                id,
                title: String::new(),
                snippet: String::new(),
                kind: "info".to_string(),
                duration_ms: None,
            })
            .collect();
        assert!(trim_notifications(&mut notifications));
        assert_eq!(notifications.len(), MAX_ACTIVE_NOTIFICATIONS);
        assert_eq!(notifications.first().unwrap().id, 5);
    }
}
