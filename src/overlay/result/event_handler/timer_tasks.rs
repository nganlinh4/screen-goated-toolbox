use crate::overlay::result::state::WINDOW_STATES;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::*;

const STREAM_RENDER_INTERVAL_MS: u32 = 80;

pub unsafe fn handle_timer(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    if wparam.0 != 3 {
        return LRESULT(0);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u32)
        .unwrap_or(0);
    let should_sync = {
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
        if update_due {
            if let Some(text) = state.pending_text.take() {
                state.full_text = text;
            }
            state.last_text_update_time = now;
        }
        update_due || streaming_ended
    };

    if should_sync {
        let visible =
            unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() };
        crate::overlay::result::scene_compositor::sync_window(hwnd, visible);
        crate::overlay::result::button_canvas::update_window_position(hwnd);
    }
    LRESULT(0)
}

#[cfg(test)]
mod tests {
    use super::STREAM_RENDER_INTERVAL_MS;

    #[test]
    fn stream_render_cadence_matches_the_visual_update_contract() {
        assert_eq!(STREAM_RENDER_INTERVAL_MS, 80);
    }
}
