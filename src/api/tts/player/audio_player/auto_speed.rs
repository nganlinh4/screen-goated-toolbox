//! Realtime TTS playback speed that follows the text and S2S backlog.

use std::sync::atomic::Ordering;

pub(super) struct AutoSpeedState {
    pub speed: u32,
}

pub(super) fn compute_realtime_auto_speed() -> AutoSpeedState {
    use crate::overlay::realtime_webview::state::{
        CURRENT_TTS_SPEED, REALTIME_TTS_AUTO_SPEED, REALTIME_TTS_SPEED,
    };

    let base_speed = REALTIME_TTS_SPEED.load(Ordering::Relaxed);
    let auto_enabled = REALTIME_TTS_AUTO_SPEED.load(Ordering::Relaxed);
    let text_queue_len = crate::api::tts::TTS_MANAGER.realtime_text_backlog();
    let backlog = crate::api::realtime_audio::s2s::backlog::current_backlog();
    let s2s_segment_backlog = backlog.segments;
    let s2s_delay_ms = backlog.delay_ms;
    let s2s_backlog_ms = backlog.audio_ms;
    let s2s_ready_backlog_ms = backlog.ready_ms;
    let text_queue_delay_ms = (text_queue_len as u32).saturating_mul(1_200);
    let s2s_ready_delay_ms = s2s_ready_backlog_ms.saturating_add(s2s_delay_ms);
    let s2s_waiting_delay_ms = s2s_backlog_ms.saturating_sub(5_000);
    let effective_delay_ms = text_queue_delay_ms
        .max(s2s_ready_delay_ms)
        .max(s2s_waiting_delay_ms);

    let target_speed = if auto_enabled {
        let boost = match effective_delay_ms {
            0..=1_800 => 0,
            1_801..=2_600 => 10,
            2_601..=3_600 => 25,
            3_601..=5_000 => 40,
            5_001..=7_000 => 55,
            _ => 75,
        };
        let cap = if text_queue_len > 0 || s2s_ready_backlog_ms > 0 {
            200
        } else if s2s_segment_backlog > 1 {
            base_speed.saturating_add(35).min(170)
        } else {
            base_speed.saturating_add(15).min(150)
        };
        (base_speed + boost).min(cap)
    } else {
        base_speed
    };

    let current_speed = CURRENT_TTS_SPEED.load(Ordering::Relaxed);
    let current_speed = if current_speed < base_speed && target_speed >= base_speed {
        base_speed
    } else {
        current_speed
    };
    let speed = if target_speed > current_speed {
        (current_speed + 15).min(target_speed)
    } else if target_speed < current_speed {
        let step_down = if s2s_segment_backlog > 0 || text_queue_len > 0 {
            2
        } else {
            5
        };
        current_speed.saturating_sub(step_down).max(target_speed)
    } else {
        current_speed
    }
    .clamp(50, 200);

    AutoSpeedState { speed }
}
