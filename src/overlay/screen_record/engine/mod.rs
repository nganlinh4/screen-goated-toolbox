mod capture_impl;
pub(crate) mod cursor;
mod cursor_sampler;
pub(crate) mod encoder_utils;
mod pump_thread;
pub mod types;

use crate::overlay::screen_record::d3d_interop::VideoProcessor;
use parking_lot::Mutex;
use std::mem::zeroed;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW,
    HDC, HMONITOR, MONITORINFOEXW,
};
use windows::core::BOOL;
use windows_capture::encoder::VideoEncoder;
use windows_capture::frame::Frame;

use encoder_utils::clone_wc_interface_to_app;
use types::VramFrame;

// Re-export all public items so external code using `engine::Foo` still works.
pub use cursor::reset_cursor_detection_state;
pub use types::{
    ACTIVE_CAPTURE_CONTROL, AUDIO_ENCODING_FINISHED, CAPTURE_ERROR, ENCODER_ACTIVE,
    ENCODING_FINISHED, IS_RECORDING, LAST_CAPTURE_FRAME_HEIGHT, LAST_CAPTURE_FRAME_WIDTH,
    LAST_RECORDING_FPS, MIC_AUDIO_ENCODING_FINISHED, MIC_AUDIO_PATH, MIC_AUDIO_START_OFFSET_MS,
    MONITOR_X, MONITOR_Y, MOUSE_POSITIONS, MonitorInfo, SHOULD_STOP, SHOULD_STOP_AUDIO,
    TARGET_HWND, VIDEO_PATH, WEBCAM_ENCODING_FINISHED, WEBCAM_VIDEO_PATH,
    WEBCAM_VIDEO_START_OFFSET_MS,
};

lazy_static::lazy_static! {
    /// Stores the CaptureControl returned by start_free_threaded so stop_recording
    /// can properly terminate the capture thread even when 0 frames arrived.
    pub static ref EXTERNAL_CAPTURE_CONTROL: parking_lot::Mutex<Option<windows_capture::capture::CaptureControl<CaptureHandler, Box<dyn std::error::Error + Send + Sync>>>> = parking_lot::Mutex::new(None);
}

pub struct CaptureHandler {
    encoder: Arc<Mutex<Option<VideoEncoder>>>,
    target_fps: u32,
    frame_interval_100ns: i64,
    start: Instant,
    cursor_sampler_stop: Arc<AtomicBool>,
    cursor_sampler_thread: Option<JoinHandle<()>>,
    next_submit_timestamp_100ns: Option<i64>,
    last_pending_frames: usize,
    frame_count: u64,
    window_arrivals: u32,
    window_enqueued: u32,
    window_dropped: u32,
    window_paced_skips: u32,
    stats_window_start: Instant,
    enc_w: u32,
    enc_h: u32,
    /// When true, frames are submitted by a background pump thread at
    /// constant FPS instead of directly from on_frame_arrived.
    is_window_capture: bool,
    /// Pre-allocated VRAM ring buffer used for zero-copy window capture pumping
    /// and GPU resize fallback when the source dimensions do not match the encoder canvas.
    vram_pool: Arc<Vec<VramFrame>>,
    /// Latest ring slot with a fully written frame for the pump thread.
    latest_ready_idx: Arc<AtomicUsize>,
    /// Next ring slot to write from the capture callback thread.
    write_idx: usize,
    /// Hardware scaler/cropper for size mismatch handling, entirely in VRAM.
    /// Stores (input_w, input_h, processor) to detect dynamic frame dimension changes.
    video_processor: Option<(u32, u32, VideoProcessor)>,
    /// D3D11 device for dynamic resource recreation.
    d3d_device: ID3D11Device,
    /// Immediate D3D11 context for GPU copy/convert operations.
    d3d_context: ID3D11DeviceContext,
    /// Signal the pump thread to stop.
    pump_stop: Arc<AtomicBool>,
    /// Frames successfully queued by the pump thread (for stats).
    pump_submitted: Arc<AtomicUsize>,
    /// Frames dropped by the pump thread due to backpressure (for stats).
    pump_dropped: Arc<AtomicUsize>,
    /// Pending-frame budget used by the encoder queue for this session.
    max_pending_frames: usize,
    /// Last implausible window frame size skipped to avoid log spam.
    last_ignored_window_frame: Option<(u32, u32)>,
    /// Avoids spamming when every staged surface is still owned by the encoder.
    vram_pool_exhausted_logged: bool,
}

impl CaptureHandler {
    fn shutdown_and_finalize(&mut self) {
        ENCODER_ACTIVE.store(false, Ordering::SeqCst);
        SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
        ACTIVE_CAPTURE_CONTROL.lock().take();

        // Stop the frame pump thread (window capture only).
        self.pump_stop.store(true, Ordering::SeqCst);

        self.cursor_sampler_stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.cursor_sampler_thread.take() {
            let _ = handle.join();
        }

        if let Some(encoder) = self.encoder.lock().take() {
            std::thread::spawn(move || {
                let audio_wait = Instant::now();
                while (!AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst)
                    || !MIC_AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst))
                    && audio_wait.elapsed().as_secs() < 5
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                if let Err(error) = encoder.finish() {
                    eprintln!("video encoder finalize error: {}", error);
                }
                ENCODING_FINISHED.store(true, Ordering::SeqCst);
            });
        }
    }

    fn next_writable_vram_slot(&self) -> Option<usize> {
        let latest_ready = self.latest_ready_idx.load(Ordering::Acquire);
        for offset in 0..self.vram_pool.len() {
            let slot = (self.write_idx + offset) % self.vram_pool.len();
            if slot == latest_ready {
                continue;
            }
            if self.vram_pool[slot].in_flight.load(Ordering::Acquire) == 0 {
                return Some(slot);
            }
        }
        None
    }

    fn stage_frame_in_vram(&mut self, frame: &Frame) -> Result<Option<usize>, String> {
        let Some(slot) = self.next_writable_vram_slot() else {
            return Ok(None);
        };
        let target_frame = &self.vram_pool[slot];
        let frame_w = frame.width();
        let frame_h = frame.height();
        let wgc_texture = unsafe { frame.as_raw_texture() };
        let wgc_texture: ID3D11Texture2D = clone_wc_interface_to_app(wgc_texture)
            .map_err(|e| format!("Failed to bridge WGC texture to app D3D type: {e}"))?;

        if frame_w == self.enc_w && frame_h == self.enc_h {
            unsafe {
                self.d3d_context
                    .CopyResource(&target_frame.texture.0, &wgc_texture);
            }
        } else {
            let needs_recreate = match &self.video_processor {
                Some((in_w, in_h, _)) => *in_w != frame_w || *in_h != frame_h,
                None => true,
            };

            if needs_recreate {
                match VideoProcessor::new_with_frame_rate(
                    &self.d3d_device,
                    &self.d3d_context,
                    frame_w,
                    frame_h,
                    self.enc_w,
                    self.enc_h,
                    self.target_fps,
                ) {
                    Ok(vp) => {
                        self.video_processor = Some((frame_w, frame_h, vp));
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to recreate GPU resize path for {}x{} -> {}x{}: {}",
                            frame_w, frame_h, self.enc_w, self.enc_h, e
                        ));
                    }
                }
            }

            if let Some((_, _, vp)) = &self.video_processor {
                vp.convert(&wgc_texture, 0, &target_frame.texture.0)?;
            } else {
                return Err("Failed to obtain VideoProcessor for resize".to_string());
            }
        }

        self.write_idx = (self.write_idx + 1) % self.vram_pool.len();
        Ok(Some(slot))
    }

    /// Log periodic capture statistics and reset the per-window counters.
    pub(crate) fn report_capture_stats(&mut self, queue_depth: usize, dropped_total: usize) {
        use types::CAPTURE_STATS_WINDOW_SECS;

        self.frame_count = self.frame_count.saturating_add(1);
        self.window_arrivals = self.window_arrivals.saturating_add(1);

        let elapsed = self.stats_window_start.elapsed().as_secs_f64();
        if elapsed >= CAPTURE_STATS_WINDOW_SECS {
            let capture_fps = self.window_arrivals as f64 / elapsed.max(0.001);
            let queued_fps = self.window_enqueued as f64 / elapsed.max(0.001);
            let pending_now = self
                .encoder
                .lock()
                .as_ref()
                .map(|encoder| encoder.pending_video_frames())
                .unwrap_or(self.last_pending_frames);
            let encoded_window = (self.last_pending_frames + self.window_enqueued as usize)
                .saturating_sub(pending_now);
            self.last_pending_frames = pending_now;
            let encoded_fps = encoded_window as f64 / elapsed.max(0.001);
            if self.is_window_capture {
                let ps = self.pump_submitted.swap(0, Ordering::Relaxed);
                let pd = self.pump_dropped.swap(0, Ordering::Relaxed);
                let pump_fps = ps as f64 / elapsed.max(0.001);
                eprintln!(
                    "[CaptureStats] backend=window(pump) wgc_fps={:.1} cached={} pump_fps={:.1} pump_submitted={} pump_dropped={} queue_depth={} dropped_total={}",
                    capture_fps, self.window_enqueued, pump_fps, ps, pd, queue_depth, dropped_total
                );
            } else {
                eprintln!(
                    "[CaptureStats] backend=display capture_fps={:.1} queue_fps={:.1} encode_fps={:.1} target_fps={} paced_skips={} queue_depth={} dropped_window={} dropped_total={}",
                    capture_fps,
                    queued_fps,
                    encoded_fps,
                    self.target_fps,
                    self.window_paced_skips,
                    queue_depth,
                    self.window_dropped,
                    dropped_total
                );
            }
            self.window_arrivals = 0;
            self.window_enqueued = 0;
            self.window_dropped = 0;
            self.window_paced_skips = 0;
            self.stats_window_start = Instant::now();
        }
    }
}

impl Drop for CaptureHandler {
    fn drop(&mut self) {
        self.shutdown_and_finalize();
    }
}

pub fn get_monitors() -> Vec<MonitorInfo> {
    let mut monitors_vec: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut monitors_vec as *mut _ as isize),
        );

        let mut monitor_infos = Vec::new();
        for (index, &hmonitor) in monitors_vec.iter().enumerate() {
            let mut info: MONITORINFOEXW = zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(hmonitor, &mut info.monitorInfo as *mut _).as_bool() {
                let rect = info.monitorInfo.rcMonitor;

                // Query the hardware refresh rate for this monitor.
                let mut devmode: DEVMODEW = zeroed();
                devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
                let hz = if EnumDisplaySettingsW(
                    windows::core::PCWSTR(info.szDevice.as_ptr()),
                    ENUM_CURRENT_SETTINGS,
                    &mut devmode,
                )
                .as_bool()
                {
                    devmode.dmDisplayFrequency
                } else {
                    60
                };

                monitor_infos.push(MonitorInfo {
                    id: index.to_string(),
                    name: format!("Display {}", index + 1),
                    x: rect.left,
                    y: rect.top,
                    width: (rect.right - rect.left) as u32,
                    height: (rect.bottom - rect.top) as u32,
                    is_primary: info.monitorInfo.dwFlags & 1 == 1,
                    hz,
                    thumbnail: None, // filled by IPC handler
                });
            }
        }
        monitor_infos
    }
}

pub unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    unsafe {
        let monitors = &mut *(lparam.0 as *mut Vec<HMONITOR>);
        monitors.push(hmonitor);
        true.into()
    }
}
