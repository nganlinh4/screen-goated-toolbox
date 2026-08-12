mod child;
mod dcomp;
mod html;
mod input;
mod parent;
pub(crate) mod protocol;
pub(crate) mod smoke;

use protocol::{HostCommand, NotificationScene, PhysicalRect, ProgressScene, RecordingScene};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use windows061::Win32::UI::HiDpi::GetDpiForSystem;
use windows061::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

pub(crate) const CHILD_FLAG: &str = "--internal-status-compositor";
static NEXT_NOTIFICATION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_CAPTURE_REQUEST: AtomicU64 = AtomicU64::new(1);

pub(crate) fn is_child_process() -> bool {
    std::env::args().any(|argument| argument == CHILD_FLAG)
}

pub(crate) fn run_child() -> anyhow::Result<()> {
    child::run()
}

pub(crate) fn warmup() {
    parent::warmup();
}

pub(crate) fn update_theme(is_dark: bool) {
    parent::SNAPSHOT.lock().unwrap().is_dark = is_dark;
    parent::send(HostCommand::Theme { is_dark });
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
        id: NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst),
        title,
        snippet,
        kind: kind.to_string(),
        duration_ms,
    };
    let rect = notification_rect();
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    snapshot.notification_rect = rect;
    snapshot.notifications.push(notification.clone());
    drop(snapshot);
    parent::send(HostCommand::NotificationAdd { rect, notification });
}

pub(crate) fn progress_upsert(title: String, snippet: String, progress: f32) {
    let progress = ProgressScene {
        title,
        snippet,
        progress: progress.clamp(0.0, 100.0),
    };
    let rect = notification_rect();
    let mut snapshot = parent::SNAPSHOT.lock().unwrap();
    snapshot.notification_rect = rect;
    snapshot.progress = Some(progress.clone());
    drop(snapshot);
    parent::send(HostCommand::ProgressUpsert { rect, progress });
}

pub(crate) fn progress_remove() {
    parent::SNAPSHOT.lock().unwrap().progress = None;
    parent::send(HostCommand::ProgressRemove);
}

pub(crate) fn selection_show(rect: PhysicalRect, text: String) {
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
        unsafe { GetSystemMetrics(windows061::Win32::UI::WindowsAndMessaging::SM_CXSCREEN) };
    let screen_height =
        unsafe { GetSystemMetrics(windows061::Win32::UI::WindowsAndMessaging::SM_CYSCREEN) };
    physical_rect(
        (screen_width - WIDTH) / 2,
        screen_height - HEIGHT - 100,
        WIDTH,
        HEIGHT,
    )
}

pub(super) fn system_dpi_scale() -> f64 {
    unsafe { GetDpiForSystem() as f64 / 96.0 }
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
