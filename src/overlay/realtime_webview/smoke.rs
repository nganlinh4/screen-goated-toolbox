use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use super::layout::{CardRect, CardRole, CompositorLayout};
use super::protocol::{CardSettings, CardText, RealtimeScene};

pub(crate) fn run() -> i32 {
    if !super::supervisor::wait_until_ready(Duration::from_secs(8)) {
        crate::debug_log::log_debug("[RealtimeSmoke] status=failed reason=renderer_not_ready");
        return 1;
    }
    super::parent::replace_scene(smoke_scene());
    for frame in 0..240 {
        super::parent::update_text(
            CardRole::Transcription,
            CardText {
                committed: "Unified realtime compositor".to_string(),
                draft: format!(" streams frame {frame} without an unbounded queue."),
            },
        );
        super::parent::update_volume((frame as f32 * 0.11).sin().abs());
        std::thread::sleep(Duration::from_millis(8));
    }
    let expected = super::parent::scene_snapshot().transcription;
    if !super::supervisor::restart_and_wait(Duration::from_secs(10)) {
        crate::debug_log::log_debug("[RealtimeSmoke] status=failed reason=restart_not_ready");
        return 1;
    }
    std::thread::sleep(Duration::from_millis(500));
    let restored = super::parent::scene_snapshot().transcription == expected;
    super::parent::set_active(false);
    crate::debug_log::log_debug(&format!(
        "[RealtimeSmoke] status={} restart_restored={restored}",
        if restored { "passed" } else { "failed" }
    ));
    if restored { 0 } else { 1 }
}

fn smoke_scene() -> RealtimeScene {
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    };
    let card_width = 520.min(width.saturating_sub(80)).max(200);
    let card_height = 240.min(height.saturating_sub(80)).max(100);
    RealtimeScene {
        active: true,
        layout: CompositorLayout {
            transcription: CardRect {
                x: x + (width - card_width) / 2,
                y: y + (height - card_height) / 2,
                width: card_width,
                height: card_height,
                visible: true,
            },
            translation: CardRect::default(),
        },
        settings: CardSettings {
            audio_source: "device".to_string(),
            target_language: "English".to_string(),
            translation_model: "google-gtx".to_string(),
            transcription_model: "gemini".to_string(),
            transcription_language: "EN".to_string(),
            font_size: 24,
        },
        tts_speed: 100,
        translation_model: "google-gtx".to_string(),
        is_dark: crate::overlay::is_dark_mode(),
        ..Default::default()
    }
}
