use super::protocol::PhysicalRect;
use std::time::Duration;

pub(crate) fn run() -> i32 {
    if !super::parent::wait_until_ready(Duration::from_secs(8)) {
        crate::debug_log::log_debug("[StatusSmoke] status=failed reason=renderer_not_ready");
        return 1;
    }

    let (screen_x, screen_y, screen_width, screen_height) = super::virtual_screen();
    let recording = centered_rect(screen_x, screen_y, screen_width, screen_height, 450, 70);
    let selection = PhysicalRect {
        x: screen_x + 80,
        y: screen_y + 80,
        width: 240,
        height: 140,
    };

    super::recording_prepare(recording);
    super::recording_show(recording);
    super::recording_update("recording", 0.45);
    super::selection_show(selection, "Select text…".to_string());
    super::add_notification(
        "Unified status compositor".to_string(),
        "Notifications, progress, selection, and recording share one renderer.".to_string(),
        "success",
        Some(20_000),
    );
    super::progress_upsert(
        "Status compositor smoke".to_string(),
        "All merged surfaces are live".to_string(),
        42.0,
    );

    for frame in 0..900 {
        let phase = frame as f32 * 0.14;
        super::recording_update("recording", 0.2 + phase.sin().abs() * 0.65);
        if frame % 12 == 0 {
            super::progress_upsert(
                "Status compositor smoke".to_string(),
                "All merged surfaces are live".to_string(),
                (frame as f32 / 1.8).min(100.0),
            );
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    super::recording_hide();
    super::selection_hide();
    super::progress_remove();
    crate::debug_log::log_debug("[StatusSmoke] status=passed");
    std::thread::sleep(Duration::from_millis(200));
    0
}

fn centered_rect(
    screen_x: i32,
    screen_y: i32,
    screen_width: i32,
    screen_height: i32,
    width: i32,
    height: i32,
) -> PhysicalRect {
    PhysicalRect {
        x: screen_x + (screen_width - width) / 2,
        y: screen_y + (screen_height - height) / 2 + 100,
        width,
        height,
    }
}
