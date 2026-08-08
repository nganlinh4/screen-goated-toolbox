use crate::overlay::result::state::WINDOW_STATES;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::*;

const STREAM_RENDER_INTERVAL_MS: u32 = 80;

struct StreamSyncSample {
    text_len: usize,
    delta_chars: usize,
    interval_ms: u32,
    streaming: bool,
}

fn chars_per_second(delta_chars: usize, interval_ms: u32) -> u32 {
    if interval_ms == 0 {
        return 0;
    }
    ((delta_chars as u64 * 1_000) / interval_ms as u64).min(u32::MAX as u64) as u32
}

pub unsafe fn handle_timer(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    if wparam.0 != 3 {
        return LRESULT(0);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u32)
        .unwrap_or(0);
    let sync_sample = {
        let mut states = WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get_mut(&(hwnd.0 as isize)) else {
            return LRESULT(0);
        };
        let streaming_ended = state.was_streaming_active && !state.is_streaming_active;
        if streaming_ended {
            state.was_streaming_active = false;
        }
        let update_due = state.pending_text.is_some()
            && (streaming_ended
                || !state.is_streaming_active
                || state.last_text_update_time == 0
                || now.wrapping_sub(state.last_text_update_time) >= STREAM_RENDER_INTERVAL_MS);
        let previous_len = state.full_text.chars().count();
        let interval_ms = if state.last_text_update_time == 0 {
            0
        } else {
            now.wrapping_sub(state.last_text_update_time)
        };
        if update_due {
            if let Some(text) = state.pending_text.take() {
                state.full_text = text;
            }
            state.last_text_update_time = now;
        }
        (update_due || streaming_ended).then(|| {
            let text_len = state.full_text.chars().count();
            StreamSyncSample {
                text_len,
                delta_chars: text_len.saturating_sub(previous_len),
                interval_ms,
                streaming: state.is_streaming_active,
            }
        })
    };

    if let Some(sample) = sync_sample {
        crate::log_info!(
            "[ResultStream] id={} streaming={} text_len={} delta_chars={} interval_ms={} chars_per_sec={}",
            hwnd.0 as isize,
            sample.streaming,
            sample.text_len,
            sample.delta_chars,
            sample.interval_ms,
            chars_per_second(sample.delta_chars, sample.interval_ms)
        );
        let visible =
            unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() };
        crate::overlay::result::scene_compositor::sync_window(hwnd, visible);
        crate::overlay::result::button_canvas::update_window_position(hwnd);
    }
    LRESULT(0)
}

#[cfg(test)]
mod tests {
    use super::{STREAM_RENDER_INTERVAL_MS, chars_per_second};

    #[test]
    fn stream_render_cadence_matches_the_visual_update_contract() {
        assert_eq!(STREAM_RENDER_INTERVAL_MS, 80);
    }

    #[test]
    fn stream_speed_telemetry_uses_the_coalesced_interval() {
        assert_eq!(chars_per_second(80, 80), 1_000);
        assert_eq!(chars_per_second(25, 50), 500);
        assert_eq!(chars_per_second(25, 0), 0);
    }
}
