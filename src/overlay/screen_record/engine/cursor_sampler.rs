use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};

use super::cursor::{any_mouse_button_down, get_cursor_type};
use super::types::{
    CURSOR_SAMPLE_MAX_FPS, CURSOR_SAMPLE_MIN_FPS, LAST_CAPTURE_FRAME_HEIGHT,
    LAST_CAPTURE_FRAME_WIDTH, MOUSE_POSITIONS, MousePosition, TARGET_HWND,
};

pub(crate) fn compute_cursor_sample_interval(target_fps: u32) -> Duration {
    let sample_fps = target_fps.clamp(CURSOR_SAMPLE_MIN_FPS, CURSOR_SAMPLE_MAX_FPS);
    Duration::from_nanos(1_000_000_000_u64 / sample_fps as u64)
}

pub(crate) fn sample_mouse_position(start: Instant) {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let is_clicked = any_mouse_button_down();
            let cursor_type = get_cursor_type(is_clicked);

            let mut offset_x = super::types::MONITOR_X;
            let mut offset_y = super::types::MONITOR_Y;

            let hwnd_val = TARGET_HWND.load(Ordering::Relaxed);
            let mut capture_width = None;
            let mut capture_height = None;
            if hwnd_val != 0 {
                let frame_width = LAST_CAPTURE_FRAME_WIDTH.load(Ordering::Relaxed);
                let frame_height = LAST_CAPTURE_FRAME_HEIGHT.load(Ordering::Relaxed);
                if frame_width > 1 && frame_height > 1 {
                    capture_width = Some(frame_width as u32);
                    capture_height = Some(frame_height as u32);
                }
                let hwnd = HWND(hwnd_val as *mut _);
                let mut rect = RECT::default();
                if DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut rect as *mut _ as *mut std::ffi::c_void,
                    std::mem::size_of::<RECT>() as u32,
                )
                .is_err()
                {
                    let _ = GetWindowRect(hwnd, &mut rect);
                }
                offset_x = rect.left;
                offset_y = rect.top;
                if capture_width.is_none() || capture_height.is_none() {
                    let width = (rect.right - rect.left).max(1) as u32;
                    let height = (rect.bottom - rect.top).max(1) as u32;
                    capture_width = Some(width);
                    capture_height = Some(height);
                }
            }

            let mouse_pos = MousePosition {
                x: point.x - offset_x,
                y: point.y - offset_y,
                timestamp: start.elapsed().as_secs_f64(),
                is_clicked,
                cursor_type,
                capture_width,
                capture_height,
            };
            MOUSE_POSITIONS.lock().push_back(mouse_pos);
        }
    }
}

pub(crate) fn spawn_cursor_sampler(
    start: Instant,
    stop: Arc<AtomicBool>,
    sample_interval: Duration,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            sample_mouse_position(start);
            std::thread::sleep(sample_interval);
        }
    })
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CaptureFlags {
    pub(crate) target_type: String,
    pub(crate) target_id: String,
    pub(crate) fps: Option<u32>,
    #[serde(default = "default_device_audio_enabled")]
    pub(crate) device_audio_enabled: bool,
    #[serde(default = "default_device_audio_mode")]
    pub(crate) device_audio_mode: String,
    #[serde(default)]
    pub(crate) device_audio_app_pid: Option<u32>,
    #[serde(default)]
    pub(crate) mic_enabled: bool,
    #[serde(default = "default_webcam_enabled")]
    pub(crate) webcam_enabled: bool,
}

fn default_device_audio_enabled() -> bool {
    true
}

fn default_device_audio_mode() -> String {
    "all".to_string()
}

fn default_webcam_enabled() -> bool {
    true
}
