use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostMessageW, SetTimer};

pub(in crate::overlay::translation_gummy) const TIMER_ID: usize = 1;
const FRAME_INTERVAL_MS: u32 = 50;

struct PendingFrame {
    epoch: usize,
    pending: Option<usize>,
    target: crate::win_types::SendHwnd,
}

impl PendingFrame {
    const fn new() -> Self {
        Self {
            epoch: 0,
            pending: None,
            target: crate::win_types::SendHwnd(HWND(std::ptr::null_mut())),
        }
    }

    fn claim(&mut self) -> Option<usize> {
        if self.pending.is_some() {
            return None;
        }
        self.epoch = self.epoch.checked_add(1)?;
        self.pending = Some(self.epoch);
        self.pending
    }

    fn finish(&mut self, ticket: usize) -> bool {
        if self.pending != Some(ticket) {
            return false;
        }
        self.pending = None;
        true
    }

    fn reset(&mut self) {
        self.pending = None;
        self.epoch = self.epoch.saturating_add(1);
    }

    fn bind(&mut self, target: HWND) {
        self.reset();
        self.target = crate::win_types::SendHwnd(target);
    }

    fn unbind(&mut self, target: HWND) {
        if self.target.0 == target {
            self.reset();
            self.target = crate::win_types::SendHwnd(HWND(std::ptr::null_mut()));
        }
    }
}

static FRAME: Mutex<PendingFrame> = Mutex::new(PendingFrame::new());

pub(super) fn normalized_level(running: bool, level: f32) -> f32 {
    if running && level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(in crate::overlay::translation_gummy) fn publish_audio_level(level: f32) {
    super::with_state(|state| {
        state.audio_level = normalized_level(state.is_running, level);
    });
}

pub(in crate::overlay::translation_gummy) fn request() {
    let mut frame = FRAME.lock().unwrap();
    let hwnd = frame.target.0;
    if hwnd.is_invalid() {
        return;
    }
    let Some(ticket) = frame.claim() else {
        return;
    };
    drop(frame);
    if unsafe {
        PostMessageW(
            Some(hwnd),
            super::super::WM_APP_METER,
            WPARAM(ticket),
            LPARAM(0),
        )
    }
    .is_err()
    {
        FRAME.lock().unwrap().finish(ticket);
    }
}

pub(in crate::overlay::translation_gummy) fn arm(hwnd: HWND, ticket: usize) {
    let frame = FRAME.lock().unwrap();
    if frame.target.0 != hwnd || frame.pending != Some(ticket) {
        return;
    }
    drop(frame);
    if unsafe { SetTimer(Some(hwnd), TIMER_ID, FRAME_INTERVAL_MS, None) } == 0 {
        FRAME.lock().unwrap().finish(ticket);
    }
}

pub(in crate::overlay::translation_gummy) fn flush(hwnd: HWND) {
    let _ = unsafe { KillTimer(Some(hwnd), TIMER_ID) };
    drain(
        &FRAME,
        hwnd,
        || super::with_state(|state| normalized_level(state.is_running, state.audio_level)),
        |level| {
            let script = format!("window.__TG_SET_AUDIO_LEVEL?.({level});");
            super::super::WEBVIEW.with(|slot| {
                if let Some(webview) = slot.borrow().as_ref() {
                    let _ = webview.evaluate_script(&script);
                }
            });
        },
    );
}

fn drain(
    frame: &Mutex<PendingFrame>,
    target: HWND,
    read: impl FnOnce() -> f32,
    deliver: impl FnOnce(f32),
) -> bool {
    let mut state = frame.lock().unwrap();
    if state.target.0 != target || state.pending.take().is_none() {
        return false;
    }
    drop(state);
    deliver(read());
    true
}

pub(in crate::overlay::translation_gummy) fn reset(hwnd: HWND) {
    let _ = unsafe { KillTimer(Some(hwnd), TIMER_ID) };
    FRAME.lock().unwrap().unbind(hwnd);
}

pub(in crate::overlay::translation_gummy) fn bind(hwnd: HWND) {
    FRAME.lock().unwrap().bind(hwnd);
}

pub(in crate::overlay::translation_gummy) fn request_state_sync() {
    let hwnd = FRAME.lock().unwrap().target.0;
    if !hwnd.is_invalid() {
        let _ =
            unsafe { PostMessageW(Some(hwnd), super::super::WM_APP_SYNC, WPARAM(0), LPARAM(0)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../parity-fixtures/translation-gummy/meter-delivery.json"
        ))
        .unwrap()
    }

    #[test]
    fn gummy_audio_bursts_keep_one_pending_frame_and_deliver_latest_level() {
        let fixture = fixture();
        assert_eq!(
            FRAME_INTERVAL_MS as u64,
            fixture["frameIntervalMs"].as_u64().unwrap()
        );
        assert_eq!(
            1000 / FRAME_INTERVAL_MS as u64,
            fixture["maxDeliveriesPerSecond"].as_u64().unwrap()
        );
        let frame = Mutex::new(PendingFrame::new());
        let latest = std::cell::Cell::new(0.0);
        let mut wakes = Vec::new();
        for index in 0..fixture["delivery"]["burstSamples"].as_u64().unwrap() {
            latest.set(index as f32);
            if let Some(ticket) = frame.lock().unwrap().claim() {
                wakes.push(ticket);
            }
        }
        assert_eq!(
            wakes.len() as u64,
            fixture["delivery"]["queuedWakes"].as_u64().unwrap()
        );
        let delivered = std::cell::Cell::new(-1.0);
        assert!(drain(
            &frame,
            HWND::default(),
            || latest.get(),
            |level| delivered.set(level)
        ));
        assert_eq!(delivered.get(), 999.0);
        assert!(!frame.lock().unwrap().finish(wakes[0]));
        assert!(frame.lock().unwrap().claim().is_some());
    }

    #[test]
    fn gummy_audio_drain_reads_current_stop_state_and_ignores_empty_wakes() {
        let frame = Mutex::new(PendingFrame::new());
        let latest = std::cell::Cell::new(0.8);
        frame.lock().unwrap().claim().unwrap();
        latest.set(0.0);
        assert!(drain(
            &frame,
            HWND::default(),
            || latest.get(),
            |value| assert_eq!(value, 0.0)
        ));
        assert!(!drain(
            &frame,
            HWND::default(),
            || panic!("stale wake read state"),
            |_| panic!("stale wake delivered")
        ));
    }

    #[test]
    fn gummy_audio_window_replacement_preserves_the_current_pending_frame() {
        let old = HWND(1usize as _);
        let current = HWND(2usize as _);
        let frame = Mutex::new(PendingFrame::new());
        let mut state = frame.lock().unwrap();
        state.bind(old);
        let old_ticket = state.claim().unwrap();
        state.bind(current);
        let current_ticket = state.claim().unwrap();
        state.unbind(old);
        assert!(!state.finish(old_ticket));
        assert_eq!(state.pending, Some(current_ticket));
        drop(state);
        assert!(!drain(
            &frame,
            old,
            || panic!("old target read"),
            |_| panic!("old target delivered")
        ));
        assert_eq!(frame.lock().unwrap().pending, Some(current_ticket));
        assert!(drain(
            &frame,
            current,
            || 0.4,
            |value| assert_eq!(value, 0.4)
        ));
    }

    #[test]
    fn gummy_audio_stale_failures_cannot_clear_replacement_frames() {
        let mut frame = PendingFrame::new();
        let old = frame.claim().unwrap();
        frame.reset();
        let current = frame.claim().unwrap();
        assert!(!frame.finish(old));
        assert_eq!(frame.pending, Some(current));
        assert!(frame.finish(current));
        frame.epoch = usize::MAX - 1;
        let last = frame.claim().unwrap();
        frame.reset();
        assert!(frame.claim().is_none());
        assert!(!frame.finish(last));
    }

    #[test]
    fn gummy_audio_values_match_shared_normalization_cases() {
        for case in fixture()["normalizationCases"].as_array().unwrap() {
            let value = match case["valueKind"].as_str() {
                Some("nan") => f32::NAN,
                Some("infinity") => f32::INFINITY,
                _ => case["value"].as_f64().unwrap() as f32,
            };
            assert_eq!(
                normalized_level(case["running"].as_bool().unwrap(), value),
                case["expected"].as_f64().unwrap() as f32
            );
        }
    }
}
