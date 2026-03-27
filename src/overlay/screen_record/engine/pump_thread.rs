use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows_capture::{SendDirectX, encoder::VideoEncoder, monitor::Monitor};

use super::types::{
    AUDIO_ENCODING_FINISHED, DEFAULT_TARGET_FPS, ENCODING_FINISHED, MIC_AUDIO_ENCODING_FINISHED,
    NO_READY_VRAM_FRAME, SHOULD_STOP, SHOULD_STOP_AUDIO, VramFrame,
};

/// Resolve the capture dimensions for a window target, with multiple fallbacks.
/// Returns (width, height, monitor_hz, target_id_print).
pub(crate) fn resolve_window_capture_size(hwnd_val: usize) -> (u32, u32, u32, usize) {
    let hwnd = HWND(hwnd_val as *mut _);
    let window = windows_capture::window::Window::from_raw_hwnd(hwnd_val as *mut std::ffi::c_void);

    let mut w = 0u32;
    let mut h = 0u32;

    // 1. Try WGC item size first, but only trust it if reasonably large.
    //    Minimized windows report 160x28 (iconic title bar size).
    if let Ok(interop) =
        windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        && let Ok(item) = unsafe { interop.CreateForWindow::<GraphicsCaptureItem>(hwnd) }
        && let Ok(size) = item.Size()
        && size.Width >= 300
        && size.Height >= 300
    {
        w = size.Width as u32;
        h = size.Height as u32;
    }

    // 2. Fallback: WINDOWPLACEMENT for restored size if currently minimized or small.
    if w == 0 || h == 0 {
        unsafe {
            let mut wp = windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT {
                length: std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>(
                ) as u32,
                ..Default::default()
            };
            if windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut wp).is_ok() {
                let pw = (wp.rcNormalPosition.right - wp.rcNormalPosition.left).abs();
                let ph = (wp.rcNormalPosition.bottom - wp.rcNormalPosition.top).abs();
                if pw >= 300 && ph >= 300 {
                    w = pw as u32;
                    h = ph as u32;
                }
            }
        }
    }

    // 3. Fallback: current window rect.
    if (w == 0 || h == 0)
        && let Ok(rect) = window.rect()
    {
        let pw = (rect.right - rect.left).abs();
        let ph = (rect.bottom - rect.top).abs();
        if pw >= 300 && ph >= 300 {
            w = pw as u32;
            h = ph as u32;
        }
    }

    // 4. Ultimate fallback: monitor size (window is hidden or completely iconic).
    if w < 300 || h < 300 {
        if let Some(monitor) = window.monitor() {
            w = monitor.width().unwrap_or(1920);
            h = monitor.height().unwrap_or(1080);
        } else {
            w = 1920;
            h = 1080;
        }
    }

    (w, h, DEFAULT_TARGET_FPS, hwnd_val)
}

/// Resolve the capture dimensions for a monitor target.
/// Returns (width, height, monitor_hz, target_id_print).
pub(crate) fn resolve_monitor_capture_size(
    monitor_index: usize,
) -> Result<(u32, u32, u32, usize), Box<dyn std::error::Error + Send + Sync>> {
    let monitor = Monitor::from_index(monitor_index + 1)?;
    let w = monitor.width()?;
    let h = monitor.height()?;
    let hz = monitor.refresh_rate().unwrap_or(DEFAULT_TARGET_FPS).max(1);
    Ok((w, h, hz, monitor_index))
}

/// Spawn the constant-FPS pump thread for window capture.
///
/// The pump thread reads the latest frame from the VRAM ring and submits it
/// to the encoder at a constant tick interval. It also handles shutdown by
/// waiting for audio engines and finalizing the encoder.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_frame_pump(
    vram_pool: Arc<Vec<VramFrame>>,
    latest_ready_idx: Arc<AtomicUsize>,
    pump_stop: Arc<AtomicBool>,
    pump_submitted: Arc<AtomicUsize>,
    pump_dropped: Arc<AtomicUsize>,
    encoder_shared: Arc<Mutex<Option<VideoEncoder>>>,
    frame_interval_100ns: i64,
    max_pending_frames: usize,
    start: Instant,
    mut pump: windows_capture::encoder::FramePump,
) {
    let tick = Duration::from_nanos((frame_interval_100ns * 100) as u64);
    eprintln!(
        "[FramePump] spawning pump thread: tick={:?} max_pending={}",
        tick, max_pending_frames
    );
    std::thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }
        eprintln!("[FramePump] pump thread started");
        let mut next_tick = start + tick;
        let mut total_submitted: u64 = 0;
        let mut total_dropped: u64 = 0;
        loop {
            // Check both the explicit pump_stop flag AND the global
            // SHOULD_STOP.  For window capture, on_frame_arrived may
            // never fire after stop is requested, so the pump thread
            // is responsible for driving the shutdown sequence.
            if pump_stop.load(Ordering::SeqCst) || SHOULD_STOP.load(Ordering::SeqCst) {
                eprintln!(
                    "[FramePump] stop detected. total_submitted={} total_dropped={}",
                    total_submitted, total_dropped
                );

                // Signal the audio engine to stop and wait for it to
                // finish flushing before sending EOF to the MF transcode.
                SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
                let audio_wait = Instant::now();
                while (!AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst)
                    || !MIC_AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst))
                    && audio_wait.elapsed().as_secs() < 5
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                eprintln!(
                    "[FramePump] audio finished={} mic_finished={}, finalizing encoder",
                    AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst),
                    MIC_AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst)
                );

                if let Some(enc) = encoder_shared.lock().take() {
                    if let Err(e) = enc.finish() {
                        eprintln!("pump thread video encoder finalize error: {}", e);
                    }
                    ENCODING_FINISHED.store(true, Ordering::SeqCst);
                }
                break;
            }

            let now = Instant::now();
            if now >= next_tick {
                let idx = latest_ready_idx.load(Ordering::Acquire);
                if idx != NO_READY_VRAM_FRAME {
                    while next_tick <= now {
                        let surface = SendDirectX::new(vram_pool[idx].surface.0.clone());
                        let release_counter = Some(vram_pool[idx].in_flight.clone());
                        if pump.submit_surface(surface, max_pending_frames, release_counter) {
                            pump_submitted.fetch_add(1, Ordering::Relaxed);
                            total_submitted += 1;
                        } else {
                            pump_dropped.fetch_add(1, Ordering::Relaxed);
                            total_dropped += 1;
                        }
                        next_tick += tick;
                    }
                } else {
                    // No frame available yet. We explicitly DO NOT advance next_tick.
                    // When the delayed first frame finally arrives, the pump will
                    // burst-submit to backfill the timeline to T=0.
                }
            }

            let sleep = if next_tick > Instant::now() {
                next_tick
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(2))
            } else {
                Duration::from_millis(1) // Keep polling gently if behind but waiting for a frame
            };
            std::thread::sleep(sleep);
        }
        eprintln!("[FramePump] pump thread exiting");
    });
}
