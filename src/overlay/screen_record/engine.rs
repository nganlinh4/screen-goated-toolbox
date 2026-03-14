use crate::overlay::screen_record::audio_engine;
use crate::overlay::screen_record::d3d_interop::{VideoProcessor, create_direct3d_surface};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem::zeroed;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use wc_windows::core::Interface as WcInterface;
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, ID3D11Device, ID3D11DeviceContext,
    ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DEVMODEW, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW,
    GetDC, GetDIBits, GetMonitorInfoW, GetObjectW, HBITMAP, HDC, HMONITOR, MONITORINFOEXW,
    ReleaseDC,
};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CURSORINFO, GetCursorInfo, GetCursorPos, GetIconInfo, GetWindowRect, ICONINFO, IDC_APPSTARTING,
    IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE,
    IDC_SIZEWE, IDC_WAIT, LoadCursorW,
};
use windows::core::{BOOL, Interface as AppInterface};
use windows_capture::{
    SendDirectX,
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    },
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    windows_bindings as wc_windows,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
    pub timestamp: f64,
    pub is_clicked: bool,
    pub cursor_type: String,
    pub capture_width: Option<u32>,
    pub capture_height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub hz: u32,
    /// JPEG data URL captured at call time; filled in by the IPC handler.
    pub thumbnail: Option<String>,
}

lazy_static::lazy_static! {
    pub static ref MOUSE_POSITIONS: Mutex<VecDeque<MousePosition>> = Mutex::new(VecDeque::new());
    pub static ref IS_RECORDING: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    /// Stores the last capture-start error so `stop_recording` can report it.
    pub static ref CAPTURE_ERROR: Mutex<Option<String>> = Mutex::new(None);
    pub static ref SHOULD_STOP: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref SHOULD_STOP_AUDIO: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref ENCODING_FINISHED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_ENCODING_FINISHED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref MIC_AUDIO_ENCODING_FINISHED: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
    pub static ref WEBCAM_ENCODING_FINISHED: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
    pub static ref ENCODER_ACTIVE: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref ACTIVE_CAPTURE_CONTROL: Mutex<Option<InternalCaptureControl>> = Mutex::new(None);
    /// Stores the CaptureControl returned by start_free_threaded so stop_recording
    /// can properly terminate the capture thread even when 0 frames arrived.
    pub static ref EXTERNAL_CAPTURE_CONTROL: Mutex<Option<CaptureControl<CaptureHandler, Box<dyn std::error::Error + Send + Sync>>>> = Mutex::new(None);
    // Last emitted cursor debug record to avoid spamming logs every frame.
    static ref LAST_CURSOR_DEBUG: Mutex<Option<(isize, String, bool, String)>> = Mutex::new(None);
    // Learned non-system custom cursor signatures that represent grab/openhand cursors.
    // Learned only when unknown cursor appears while clicked=true.
    static ref CUSTOM_GRAB_SIGNATURES: Mutex<HashSet<String>> = {
        Mutex::new(load_grab_signatures())
    };
    // Runtime-computed signatures for the current machine's system cursor shapes.
    // This catches apps/games that clone a system cursor into a private handle.
    static ref SYSTEM_CURSOR_SIGNATURES: HashMap<String, &'static str> = load_system_cursor_signatures();
    // Resolve system cursor handles once; avoids repeated LoadCursorW calls per sample.
    static ref SYSTEM_CURSOR_HANDLES: SystemCursorHandles = load_system_cursor_handles();
    // Cache cursor_signature() results by HCURSOR raw pointer value.
    // Windows reuses cursor handles for the lifetime of a process, so a given
    // pointer always maps to the same bitmap metadata.  Cleared on recording start.
    pub static ref CURSOR_SIGNATURE_CACHE: Mutex<HashMap<isize, String>> = Mutex::new(HashMap::new());
    // Most recent unknown cursor seen while no mouse button was held. Used to
    // safely learn custom drag/grab cursors only when the shape changed under drag.
    static ref LAST_UNKNOWN_RELEASED_SIGNATURE: Mutex<Option<(String, Instant)>> = Mutex::new(None);
    // Set SCREEN_RECORD_CURSOR_DEBUG=1 to enable verbose cursor classification logs.
    static ref CURSOR_DEBUG_ENABLED: bool = {
        std::env::var("SCREEN_RECORD_CURSOR_DEBUG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    };
}

pub static VIDEO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static AUDIO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static MIC_AUDIO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static WEBCAM_VIDEO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static MIC_AUDIO_START_OFFSET_MS: AtomicU64 = AtomicU64::new(u64::MAX);
pub static WEBCAM_VIDEO_START_OFFSET_MS: AtomicU64 = AtomicU64::new(u64::MAX);
/// FPS the most recent recording was actually encoded at. Used by stop_recording
/// so the frontend can show the correct "Match Original" option in the export UI.
pub static LAST_RECORDING_FPS: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);
pub static mut MONITOR_X: i32 = 0;
pub static mut MONITOR_Y: i32 = 0;
/// Dynamically track target window so cursor math stays accurate if the window moves.
pub static TARGET_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub static LAST_CAPTURE_FRAME_WIDTH: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
pub static LAST_CAPTURE_FRAME_HEIGHT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

const DEFAULT_GRAB_SIGNATURE: &str = "hot(13,13)|mask(32x32)|color(32x32)|mono(0)";
const DEFAULT_TARGET_FPS: u32 = 60;
const ENCODER_MAX_PENDING_FRAMES: usize = 12;
const MAX_CATCHUP_SUBMITS_PER_CALLBACK: u32 = 6;
const TIMESTAMP_RESYNC_THRESHOLD_100NS: i64 = 10_000_000;
const CAPTURE_STATS_WINDOW_SECS: f64 = 1.0;
const CURSOR_SAMPLE_MIN_FPS: u32 = 30;
const CURSOR_SAMPLE_MAX_FPS: u32 = 120;
const CURSOR_GRAB_LEARN_WINDOW_MS: u64 = 1_000;
const NO_READY_VRAM_FRAME: usize = usize::MAX;
const MF_HW_ACCEL_AUTO_PIXELS_PER_SEC_THRESHOLD: u64 = 120_000_000;
const MIN_VALID_WINDOW_FRAME_DIM: u32 = 300;
const WINDOW_CAPTURE_QUEUE_TARGET_MS: usize = 350;
const WINDOW_CAPTURE_MAX_PENDING_FRAMES: usize = 48;
const WINDOW_CAPTURE_VRAM_POOL_MIN_FRAMES: usize = 6;
const WINDOW_CAPTURE_VRAM_POOL_MAX_FRAMES: usize = 12;

#[derive(Clone, Copy)]
struct SystemCursorHandles {
    arrow: isize,
    ibeam: isize,
    wait: isize,
    appstarting: isize,
    cross: isize,
    hand: isize,
    size_all: isize,
    size_ns: isize,
    size_we: isize,
    size_nwse: isize,
    size_nesw: isize,
}

struct VramFrame {
    texture: SendDirectX<ID3D11Texture2D>,
    surface: SendDirectX<wc_windows::Graphics::DirectX::Direct3D11::IDirect3DSurface>,
    in_flight: Arc<AtomicUsize>,
}

// The ring buffer is shared read-only across threads. Actual mutation happens via
// the D3D11 API using the texture handles, coordinated by the capture callback and
// pump index atomics.
unsafe impl Sync for VramFrame {}

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
}

fn clone_wc_interface_to_app<TFrom, TTo>(src: &TFrom) -> Result<TTo, String>
where
    TFrom: WcInterface,
    TTo: AppInterface,
{
    let raw = src.as_raw();
    let borrowed = unsafe { <TTo as AppInterface>::from_raw_borrowed(&raw) }
        .ok_or_else(|| "null COM pointer".to_string())?;
    Ok(borrowed.clone())
}

fn clone_app_interface_to_wc<TFrom, TTo>(src: &TFrom) -> Result<TTo, String>
where
    TFrom: AppInterface,
    TTo: WcInterface,
{
    let raw = src.as_raw();
    let borrowed = unsafe { <TTo as WcInterface>::from_raw_borrowed(&raw) }
        .ok_or_else(|| "null COM pointer".to_string())?;
    Ok(borrowed.clone())
}

fn select_target_fps(monitor_hz: u32) -> u32 {
    // Prefer exact monitor divisors in the 50-60fps export band.
    // Example: 165Hz -> 55fps (exact), which removes recurring pacing drift.
    for candidate in (50..=60).rev() {
        if monitor_hz.is_multiple_of(candidate) {
            return candidate;
        }
    }

    DEFAULT_TARGET_FPS
}

fn compute_window_max_pending_frames(target_fps: u32) -> usize {
    let target_fps = target_fps.max(1) as usize;
    let buffered_frames = (target_fps * WINDOW_CAPTURE_QUEUE_TARGET_MS).div_ceil(1000);
    buffered_frames.clamp(
        ENCODER_MAX_PENDING_FRAMES,
        WINDOW_CAPTURE_MAX_PENDING_FRAMES,
    )
}

fn compute_window_vram_pool_frames(max_pending_frames: usize) -> usize {
    max_pending_frames.div_ceil(2).clamp(
        WINDOW_CAPTURE_VRAM_POOL_MIN_FRAMES,
        WINDOW_CAPTURE_VRAM_POOL_MAX_FRAMES,
    )
}

fn should_ignore_window_frame(frame_w: u32, frame_h: u32) -> bool {
    frame_w < MIN_VALID_WINDOW_FRAME_DIM || frame_h < MIN_VALID_WINDOW_FRAME_DIM
}

fn mf_hw_accel_override() -> Option<bool> {
    match std::env::var("SCREEN_RECORD_MF_HW_ACCEL") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            Some(matches!(normalized.as_str(), "1" | "true" | "yes" | "on"))
        }
        Err(_) => None,
    }
}

fn should_prefer_mf_hw_accel(target_type: &str, target_fps: u32, width: u32, height: u32) -> bool {
    if let Some(explicit) = mf_hw_accel_override() {
        return explicit;
    }

    let pixels_per_sec = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(target_fps.max(1) as u64);

    target_type == "window" || pixels_per_sec >= MF_HW_ACCEL_AUTO_PIXELS_PER_SEC_THRESHOLD
}

struct ScopedMfHwAccelEnv {
    previous: Option<std::ffi::OsString>,
}

impl ScopedMfHwAccelEnv {
    fn set(enabled: bool) -> Self {
        let key = "SCREEN_RECORD_MF_HW_ACCEL";
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, if enabled { "1" } else { "0" });
        }
        Self { previous }
    }
}

impl Drop for ScopedMfHwAccelEnv {
    fn drop(&mut self) {
        let key = "SCREEN_RECORD_MF_HW_ACCEL";
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

struct MfEncoderCreateConfig<'a> {
    enc_w: u32,
    enc_h: u32,
    target_fps: u32,
    final_bitrate: u32,
    sample_rate: u32,
    channels: u32,
    video_path: &'a std::path::Path,
    prefer_hw: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EncoderCanvas {
    width: u32,
    height: u32,
}

fn exact_encoder_canvas(width: u32, height: u32) -> EncoderCanvas {
    EncoderCanvas {
        width: width.max(128) & !1,
        height: height.max(128) & !1,
    }
}

fn aligned_encoder_canvas(width: u32, height: u32) -> EncoderCanvas {
    EncoderCanvas {
        width: (width.max(128) + 15) & !15,
        height: (height.max(128) + 15) & !15,
    }
}

fn create_video_encoder_with_mf_mode(
    config: MfEncoderCreateConfig<'_>,
) -> Result<VideoEncoder, Box<dyn std::error::Error + Send + Sync>> {
    let _env_scope = ScopedMfHwAccelEnv::set(config.prefer_hw);
    let encoder = VideoEncoder::new(
        VideoSettingsBuilder::new(config.enc_w, config.enc_h)
            .sub_type(VideoSettingsSubType::H264)
            .bitrate(config.final_bitrate)
            .frame_rate(config.target_fps),
        AudioSettingsBuilder::new()
            .sample_rate(config.sample_rate)
            .channel_count(config.channels)
            .bitrate(192_000)
            .disabled(false),
        ContainerSettingsBuilder::new(),
        config.video_path,
    )?;
    Ok(encoder)
}

fn create_video_encoder_with_canvas_fallback(
    config: MfEncoderCreateConfig<'_>,
    capture_width: u32,
    capture_height: u32,
) -> Result<(VideoEncoder, EncoderCanvas), Box<dyn std::error::Error + Send + Sync>> {
    let exact_canvas = exact_encoder_canvas(capture_width, capture_height);
    let aligned_canvas = aligned_encoder_canvas(capture_width, capture_height);
    let canvases = if exact_canvas == aligned_canvas {
        [Some(exact_canvas), None]
    } else {
        [Some(exact_canvas), Some(aligned_canvas)]
    };
    let mut last_error = None;

    for canvas in canvases.into_iter().flatten() {
        match create_video_encoder_with_mf_mode(MfEncoderCreateConfig {
            enc_w: canvas.width,
            enc_h: canvas.height,
            target_fps: config.target_fps,
            final_bitrate: config.final_bitrate,
            sample_rate: config.sample_rate,
            channels: config.channels,
            video_path: config.video_path,
            prefer_hw: config.prefer_hw,
        }) {
            Ok(encoder) => {
                if canvas != exact_canvas {
                    eprintln!(
                        "[CaptureBackend] Exact encoder canvas {}x{} rejected; using 16-aligned fallback {}x{}",
                        exact_canvas.width, exact_canvas.height, canvas.width, canvas.height
                    );
                }
                return Ok((encoder, canvas));
            }
            Err(error) => {
                if canvas == exact_canvas && aligned_canvas != exact_canvas {
                    eprintln!(
                        "[CaptureBackend] Exact encoder canvas {}x{} init failed; retrying 16-aligned {}x{}: {}",
                        canvas.width,
                        canvas.height,
                        aligned_canvas.width,
                        aligned_canvas.height,
                        error
                    );
                }
                last_error = Some(error);
            }
        }
    }

    Err(last_error.expect("encoder canvas attempts must include at least one candidate"))
}

fn get_cursor_type(is_clicked: bool) -> String {
    unsafe {
        let mut cursor_info: CURSORINFO = std::mem::zeroed();
        cursor_info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;

        if GetCursorInfo(&mut cursor_info).is_ok() && cursor_info.flags.0 != 0 {
            let current_handle = cursor_info.hCursor.0;
            let current_handle_key = current_handle as isize;
            let handles = *SYSTEM_CURSOR_HANDLES;
            let mut signature = "system".to_string();
            let cursor_type = if current_handle_key == handles.arrow {
                clear_last_unknown_released_signature();
                "default".to_string()
            } else if current_handle_key == handles.ibeam {
                clear_last_unknown_released_signature();
                "text".to_string()
            } else if current_handle_key == handles.wait {
                clear_last_unknown_released_signature();
                "wait".to_string()
            } else if current_handle_key == handles.appstarting {
                clear_last_unknown_released_signature();
                "appstarting".to_string()
            } else if current_handle_key == handles.cross {
                clear_last_unknown_released_signature();
                "crosshair".to_string()
            } else if current_handle_key == handles.size_all {
                clear_last_unknown_released_signature();
                "move".to_string()
            } else if current_handle_key == handles.size_ns {
                clear_last_unknown_released_signature();
                "resize_ns".to_string()
            } else if current_handle_key == handles.size_we {
                clear_last_unknown_released_signature();
                "resize_we".to_string()
            } else if current_handle_key == handles.size_nwse {
                clear_last_unknown_released_signature();
                "resize_nwse".to_string()
            } else if current_handle_key == handles.size_nesw {
                clear_last_unknown_released_signature();
                "resize_nesw".to_string()
            } else if current_handle_key == handles.hand {
                clear_last_unknown_released_signature();
                "pointer".to_string()
            } else {
                signature = {
                    let mut cache = CURSOR_SIGNATURE_CACHE.lock();
                    if let Some(cached) = cache.get(&current_handle_key) {
                        cached.clone()
                    } else {
                        let sig = cursor_signature(cursor_info.hCursor)
                            .unwrap_or_else(|| "n/a".to_string());
                        cache.insert(current_handle_key, sig.clone());
                        sig
                    }
                };
                if let Some(mapped) = SYSTEM_CURSOR_SIGNATURES.get(&signature) {
                    clear_last_unknown_released_signature();
                    (*mapped).to_string()
                } else if CUSTOM_GRAB_SIGNATURES.lock().contains(&signature) {
                    clear_last_unknown_released_signature();
                    "grab".to_string()
                } else if should_learn_custom_grab_signature(&signature, is_clicked) {
                    let should_persist = {
                        let mut set = CUSTOM_GRAB_SIGNATURES.lock();
                        set.insert(signature.clone())
                    };
                    if should_persist {
                        println!("[CursorDetect] learn-grab-signature {}", signature);
                        persist_grab_signatures();
                    }
                    clear_last_unknown_released_signature();
                    "grab".to_string()
                } else {
                    if !is_clicked {
                        remember_unknown_released_signature(&signature);
                    }
                    "other".to_string()
                }
            };

            // Debug logging: emit only when cursor handle/type/click-state changes.
            let mut last = LAST_CURSOR_DEBUG.lock();
            let changed = match &*last {
                Some((h, t, c, s)) => {
                    *h != current_handle_key
                        || t != &cursor_type
                        || *c != is_clicked
                        || s != &signature
                }
                None => true,
            };
            if changed {
                if *CURSOR_DEBUG_ENABLED {
                    println!(
                        "[CursorDetect] handle=0x{:X} type={} clicked={} sig={}",
                        current_handle_key as usize, cursor_type, is_clicked, signature
                    );
                }
                *last = Some((
                    current_handle_key,
                    cursor_type.clone(),
                    is_clicked,
                    signature,
                ));
            }

            cursor_type
        } else {
            "default".to_string()
        }
    }
}

fn load_system_cursor_handle(cursor_id: windows::core::PCWSTR) -> isize {
    unsafe {
        LoadCursorW(None, cursor_id)
            .map(|cursor| cursor.0 as isize)
            .unwrap_or_default()
    }
}

fn load_system_cursor_handles() -> SystemCursorHandles {
    SystemCursorHandles {
        arrow: load_system_cursor_handle(IDC_ARROW),
        ibeam: load_system_cursor_handle(IDC_IBEAM),
        wait: load_system_cursor_handle(IDC_WAIT),
        appstarting: load_system_cursor_handle(IDC_APPSTARTING),
        cross: load_system_cursor_handle(IDC_CROSS),
        hand: load_system_cursor_handle(IDC_HAND),
        size_all: load_system_cursor_handle(IDC_SIZEALL),
        size_ns: load_system_cursor_handle(IDC_SIZENS),
        size_we: load_system_cursor_handle(IDC_SIZEWE),
        size_nwse: load_system_cursor_handle(IDC_SIZENWSE),
        size_nesw: load_system_cursor_handle(IDC_SIZENESW),
    }
}

fn load_system_cursor_signatures() -> HashMap<String, &'static str> {
    let handles = *SYSTEM_CURSOR_HANDLES;
    let mut signatures = HashMap::new();
    for (handle, cursor_type) in [
        (handles.arrow, "default"),
        (handles.ibeam, "text"),
        (handles.wait, "wait"),
        (handles.appstarting, "appstarting"),
        (handles.cross, "crosshair"),
        (handles.hand, "pointer"),
        (handles.size_all, "move"),
        (handles.size_ns, "resize_ns"),
        (handles.size_we, "resize_we"),
        (handles.size_nwse, "resize_nwse"),
        (handles.size_nesw, "resize_nesw"),
    ] {
        if handle == 0 {
            continue;
        }
        if let Some(signature) = cursor_signature(windows::Win32::UI::WindowsAndMessaging::HCURSOR(
            handle as *mut _,
        )) {
            signatures.insert(signature, cursor_type);
        }
    }
    signatures
}

fn grab_signatures_file_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    base.join("screen-goated-toolbox")
        .join("recordings")
        .join("cursor_grab_signatures.json")
}

fn load_grab_signatures() -> HashSet<String> {
    let mut out = HashSet::new();
    out.insert(DEFAULT_GRAB_SIGNATURE.to_string());

    let path = grab_signatures_file_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return out;
    };
    let Ok(saved) = serde_json::from_str::<Vec<String>>(&text) else {
        return out;
    };
    for sig in saved {
        if !sig.trim().is_empty() {
            out.insert(sig);
        }
    }
    out
}

fn persist_grab_signatures() {
    let path = grab_signatures_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let signatures = {
        let set = CUSTOM_GRAB_SIGNATURES.lock();
        let mut v: Vec<String> = set.iter().cloned().collect();
        v.sort();
        v
    };
    if let Ok(payload) = serde_json::to_string_pretty(&signatures) {
        let _ = fs::write(path, payload);
    }
}

fn clear_last_unknown_released_signature() {
    *LAST_UNKNOWN_RELEASED_SIGNATURE.lock() = None;
}

pub fn reset_cursor_detection_state() {
    CURSOR_SIGNATURE_CACHE.lock().clear();
    clear_last_unknown_released_signature();
    *LAST_CURSOR_DEBUG.lock() = None;
}

fn remember_unknown_released_signature(signature: &str) {
    if signature == "n/a" {
        return;
    }
    *LAST_UNKNOWN_RELEASED_SIGNATURE.lock() = Some((signature.to_string(), Instant::now()));
}

fn should_learn_custom_grab_signature(signature: &str, is_clicked: bool) -> bool {
    if !is_clicked || signature == "n/a" {
        return false;
    }
    let last = LAST_UNKNOWN_RELEASED_SIGNATURE.lock();
    let Some((released_signature, seen_at)) = last.as_ref() else {
        return false;
    };
    seen_at.elapsed() <= Duration::from_millis(CURSOR_GRAB_LEARN_WINDOW_MS)
        && released_signature != signature
}

fn hash_bitmap_bits(hbitmap: HBITMAP, bitmap: &BITMAP) -> Option<String> {
    let width = bitmap.bmWidth.max(1);
    let height = bitmap.bmHeight.unsigned_abs().max(1);
    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.0.is_null() {
            return None;
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.0.is_null() {
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let lines = GetDIBits(
            mem_dc,
            hbitmap,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        if lines == 0 {
            return None;
        }

        let mut hasher = DefaultHasher::new();
        pixels.hash(&mut hasher);
        Some(format!(
            "{}x{}@{}bpp#{:016x}",
            bitmap.bmWidth,
            bitmap.bmHeight,
            bitmap.bmBitsPixel,
            hasher.finish()
        ))
    }
}

fn cursor_signature(handle: windows::Win32::UI::WindowsAndMessaging::HCURSOR) -> Option<String> {
    unsafe {
        let mut icon_info: ICONINFO = zeroed();
        if GetIconInfo(handle.into(), &mut icon_info).is_err() {
            return None;
        }

        let mut mask_bm: BITMAP = zeroed();
        let mut color_bm: BITMAP = zeroed();

        if !icon_info.hbmMask.0.is_null() {
            let _ = GetObjectW(
                icon_info.hbmMask.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut mask_bm as *mut BITMAP).cast()),
            );
        }
        if !icon_info.hbmColor.0.is_null() {
            let _ = GetObjectW(
                icon_info.hbmColor.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut color_bm as *mut BITMAP).cast()),
            );
        }

        let mask_signature = if !icon_info.hbmMask.0.is_null() {
            hash_bitmap_bits(icon_info.hbmMask, &mask_bm).unwrap_or_else(|| "n/a".to_string())
        } else {
            "none".to_string()
        };
        let color_signature = if !icon_info.hbmColor.0.is_null() {
            hash_bitmap_bits(icon_info.hbmColor, &color_bm).unwrap_or_else(|| "n/a".to_string())
        } else {
            "none".to_string()
        };

        if !icon_info.hbmMask.0.is_null() {
            let _ = DeleteObject(icon_info.hbmMask.into());
        }
        if !icon_info.hbmColor.0.is_null() {
            let _ = DeleteObject(icon_info.hbmColor.into());
        }

        let base_signature = format!(
            "hot({},{})|mask({}x{})|color({}x{})|mono({})",
            icon_info.xHotspot,
            icon_info.yHotspot,
            mask_bm.bmWidth,
            mask_bm.bmHeight,
            color_bm.bmWidth,
            color_bm.bmHeight,
            if icon_info.hbmColor.0.is_null() { 1 } else { 0 }
        );

        Some(format!(
            "{}|mask_bits({})|color_bits({})",
            base_signature, mask_signature, color_signature
        ))
    }
}

fn any_mouse_button_down() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_MBUTTON.0 as i32) as u16 & 0x8000) != 0
    }
}

fn compute_cursor_sample_interval(target_fps: u32) -> Duration {
    let sample_fps = target_fps.clamp(CURSOR_SAMPLE_MIN_FPS, CURSOR_SAMPLE_MAX_FPS);
    Duration::from_nanos(1_000_000_000_u64 / sample_fps as u64)
}

fn sample_mouse_position(start: Instant) {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let is_clicked = any_mouse_button_down();
            let cursor_type = get_cursor_type(is_clicked);

            let mut offset_x = MONITOR_X;
            let mut offset_y = MONITOR_Y;

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

fn spawn_cursor_sampler(
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
struct CaptureFlags {
    target_type: String,
    target_id: String,
    fps: Option<u32>,
    #[serde(default = "default_device_audio_enabled")]
    device_audio_enabled: bool,
    #[serde(default = "default_device_audio_mode")]
    device_audio_mode: String,
    #[serde(default)]
    device_audio_app_pid: Option<u32>,
    #[serde(default)]
    mic_enabled: bool,
    #[serde(default = "default_webcam_enabled")]
    webcam_enabled: bool,
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

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = String;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = serde_json::from_str::<CaptureFlags>(&ctx.flags).unwrap_or_else(|e| {
            // Backward compatibility for legacy plain monitor-id flags.
            eprintln!(
                "[CaptureHandler::new] flags JSON parse failed ({e}), raw={:?}",
                ctx.flags
            );
            CaptureFlags {
                target_type: "monitor".to_string(),
                target_id: ctx.flags.clone(),
                fps: None,
                device_audio_enabled: true,
                device_audio_mode: "all".to_string(),
                device_audio_app_pid: None,
                mic_enabled: false,
                webcam_enabled: true,
            }
        });
        eprintln!(
            "[CaptureHandler::new] target_type={:?}, target_id={:?}",
            flags.target_type, flags.target_id
        );

        let (width, height, monitor_hz, target_id_print) = if flags.target_type == "window" {
            let hwnd_val = flags.target_id.parse::<usize>().unwrap_or(0);
            let hwnd = HWND(hwnd_val as *mut _);
            let window =
                windows_capture::window::Window::from_raw_hwnd(hwnd_val as *mut std::ffi::c_void);

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
                        length: std::mem::size_of::<
                            windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT,
                        >() as u32,
                        ..Default::default()
                    };
                    if windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut wp)
                        .is_ok()
                    {
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
        } else {
            let monitor_index = flags.target_id.parse::<usize>().unwrap_or(0);
            let monitor = Monitor::from_index(monitor_index + 1)?;
            let w = monitor.width()?;
            let h = monitor.height()?;
            let hz = monitor.refresh_rate().unwrap_or(DEFAULT_TARGET_FPS).max(1);
            (w, h, hz, monitor_index)
        };

        // Prefer the exact even capture size. Some MF encoder/device combinations only
        // accept 16-aligned canvases, so we retry with that fallback on init failure.
        let preferred_canvas = exact_encoder_canvas(width, height);

        let app_data_dir = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("screen-goated-toolbox")
            .join("recordings");

        std::fs::create_dir_all(&app_data_dir)?;

        let video_path = app_data_dir.join(format!(
            "recording_{}.mp4",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let mic_audio_path = video_path.with_file_name(format!(
            "{}_mic.wav",
            video_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("recording")
        ));
        let webcam_video_path = video_path.with_file_name(format!(
            "{}_webcam.mp4",
            video_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("recording")
        ));

        *VIDEO_PATH.lock().unwrap() = Some(video_path.to_string_lossy().to_string());
        *AUDIO_PATH.lock().unwrap() = Some(video_path.to_string_lossy().to_string());
        *MIC_AUDIO_PATH.lock().unwrap() = None;
        *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
        MIC_AUDIO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);
        WEBCAM_VIDEO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);

        let target_fps = flags.fps.unwrap_or_else(|| select_target_fps(monitor_hz));
        *LAST_RECORDING_FPS.lock().unwrap() = Some(target_fps);
        LAST_CAPTURE_FRAME_WIDTH.store(width as usize, Ordering::Relaxed);
        LAST_CAPTURE_FRAME_HEIGHT.store(height as usize, Ordering::Relaxed);
        let frame_interval_100ns = 10_000_000 / target_fps as i64;

        // DYNAMIC BITRATE CALCULATION
        // Prior 0.35 bpp target could trigger intermittent MediaFoundation HW encoder
        // backpressure during heavy gameplay. Use a more stable 0.22 bpp target.
        // 1920x1080 @ 60fps = ~27 Mbps
        // 2560x1440 @ 60fps = ~48 Mbps
        // 3840x2160 @ 60fps = ~109 Mbps
        let pixel_count = preferred_canvas.width as u64 * preferred_canvas.height as u64;
        let target_bitrate = (pixel_count as f64 * target_fps as f64 * 0.22) as u32;

        // Keep a quality floor while capping peak encoder pressure.
        let final_bitrate = target_bitrate.clamp(8_000_000, 80_000_000);

        let (sample_rate, channels) = audio_engine::get_default_audio_config();
        let mf_hw_preferred = should_prefer_mf_hw_accel(
            &flags.target_type,
            target_fps,
            preferred_canvas.width,
            preferred_canvas.height,
        );
        let (encoder, canvas, encoder_uses_hw) = match create_video_encoder_with_canvas_fallback(
            MfEncoderCreateConfig {
                enc_w: preferred_canvas.width,
                enc_h: preferred_canvas.height,
                target_fps,
                final_bitrate,
                sample_rate,
                channels,
                video_path: &video_path,
                prefer_hw: mf_hw_preferred,
            },
            width,
            height,
        ) {
            Ok((encoder, canvas)) => (encoder, canvas, mf_hw_preferred),
            Err(error) if mf_hw_preferred && mf_hw_accel_override().is_none() => {
                eprintln!(
                    "[CaptureBackend] HW encoder init failed, retrying software path: {}",
                    error
                );
                let (encoder, canvas) = create_video_encoder_with_canvas_fallback(
                    MfEncoderCreateConfig {
                        enc_w: preferred_canvas.width,
                        enc_h: preferred_canvas.height,
                        target_fps,
                        final_bitrate,
                        sample_rate,
                        channels,
                        video_path: &video_path,
                        prefer_hw: false,
                    },
                    width,
                    height,
                )?;
                (encoder, canvas, false)
            }
            Err(error) => return Err(error),
        };
        let enc_w = canvas.width;
        let enc_h = canvas.height;
        let audio_handle = encoder.create_audio_handle();
        println!(
            "Initializing VideoEncoder: {}x{} @ {}fps (Hz={}), Codec: H264 (MediaFoundation {}), Bitrate: {} Mbps, TargetType: {}, TargetID: {}",
            enc_w,
            enc_h,
            target_fps,
            monitor_hz,
            if encoder_uses_hw { "HW" } else { "SW" },
            final_bitrate / 1_000_000,
            flags.target_type,
            target_id_print
        );

        SHOULD_STOP_AUDIO.store(false, Ordering::SeqCst);
        AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
        MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        let device_audio_source = if !flags.device_audio_enabled {
            audio_engine::DeviceAudioCaptureSource::Disabled
        } else if flags.device_audio_mode == "app" {
            flags
                .device_audio_app_pid
                .map(audio_engine::DeviceAudioCaptureSource::SingleApp)
                .unwrap_or(audio_engine::DeviceAudioCaptureSource::SystemOutput)
        } else {
            audio_engine::DeviceAudioCaptureSource::SystemOutput
        };
        let start = Instant::now();
        audio_engine::record_audio(
            audio_handle,
            start,
            SHOULD_STOP_AUDIO.clone(),
            AUDIO_ENCODING_FINISHED.clone(),
            device_audio_source,
        );
        if flags.mic_enabled {
            MIC_AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
            match audio_engine::record_mic_audio_sidecar(
                mic_audio_path.to_string_lossy().to_string(),
                start,
                SHOULD_STOP_AUDIO.clone(),
                MIC_AUDIO_ENCODING_FINISHED.clone(),
                &MIC_AUDIO_START_OFFSET_MS,
            ) {
                Ok(()) => {
                    *MIC_AUDIO_PATH.lock().unwrap() =
                        Some(mic_audio_path.to_string_lossy().to_string());
                }
                Err(error) => {
                    MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
                    *MIC_AUDIO_PATH.lock().unwrap() = None;
                    eprintln!("[MicCapture] {}", error);
                }
            }
        } else {
            MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
            *MIC_AUDIO_PATH.lock().unwrap() = None;
        }
        if flags.webcam_enabled {
            WEBCAM_ENCODING_FINISHED.store(false, Ordering::SeqCst);
            match super::webcam_capture::record_webcam_video_sidecar(
                webcam_video_path.to_string_lossy().to_string(),
                start,
                SHOULD_STOP_AUDIO.clone(),
                WEBCAM_ENCODING_FINISHED.clone(),
                &WEBCAM_VIDEO_START_OFFSET_MS,
            ) {
                Ok(()) => {
                    *WEBCAM_VIDEO_PATH.lock().unwrap() =
                        Some(webcam_video_path.to_string_lossy().to_string());
                }
                Err(error) => {
                    WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
                    *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
                    eprintln!("[WebcamCapture] {}", error);
                }
            }
        } else {
            WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
            *WEBCAM_VIDEO_PATH.lock().unwrap() = None;
        }

        ENCODER_ACTIVE.store(true, Ordering::SeqCst);
        ENCODING_FINISHED.store(false, Ordering::SeqCst);
        let cursor_sampler_stop = Arc::new(AtomicBool::new(false));

        let is_window_capture = flags.target_type == "window";
        let app_d3d_device: ID3D11Device = clone_wc_interface_to_app(&ctx.device)
            .map_err(|e| format!("Failed to bridge capture D3D11 device: {e}"))?;
        // Enable D3D11 multithread protection: the WGC on_frame_arrived callback fires
        // from a background OS thread while the pump thread concurrently accesses the
        // same D3D11 Immediate Context. Without this, CopyResource races cause
        // DXGI_ERROR_DEVICE_REMOVED or silent GPU corruption.
        {
            let mt: ID3D11Multithread = app_d3d_device
                .cast()
                .map_err(|e| format!("QI ID3D11Multithread (capture): {e}"))?;
            unsafe {
                let _ = mt.SetMultithreadProtected(true);
            }
        }
        let app_d3d_context: ID3D11DeviceContext =
            clone_wc_interface_to_app(&ctx.device_context)
                .map_err(|e| format!("Failed to bridge capture D3D11 context: {e}"))?;
        let max_pending_frames = if is_window_capture {
            compute_window_max_pending_frames(target_fps)
        } else {
            ENCODER_MAX_PENDING_FRAMES
        };
        let window_vram_pool_frames = if is_window_capture {
            compute_window_vram_pool_frames(max_pending_frames)
        } else {
            3
        };
        let mut vram_frames = Vec::with_capacity(window_vram_pool_frames);
        for _ in 0..window_vram_pool_frames {
            let texture = VideoProcessor::create_texture(
                &app_d3d_device,
                enc_w,
                enc_h,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
            )
            .map_err(|e| format!("Failed to create VRAM ring texture: {e}"))?;
            let surface = create_direct3d_surface(&texture)
                .map_err(|e| format!("Failed to create WinRT surface for VRAM ring: {e}"))?;
            let surface = clone_app_interface_to_wc(&surface)
                .map_err(|e| format!("Failed to bridge WinRT surface to encoder type: {e}"))?;
            vram_frames.push(VramFrame {
                texture: SendDirectX::new(texture),
                surface: SendDirectX::new(surface),
                in_flight: Arc::new(AtomicUsize::new(0)),
            });
        }
        let vram_pool = Arc::new(vram_frames);
        let latest_ready_idx = Arc::new(AtomicUsize::new(NO_READY_VRAM_FRAME));
        let video_processor = if width != enc_w || height != enc_h {
            match VideoProcessor::new_with_frame_rate(
                &app_d3d_device,
                &app_d3d_context,
                width,
                height,
                enc_w,
                enc_h,
                target_fps,
            ) {
                Ok(vp) => Some((width, height, vp)),
                Err(e) => {
                    eprintln!(
                        "[CaptureHandler] GPU resize path unavailable for {}x{} -> {}x{}: {}",
                        width, height, enc_w, enc_h, e
                    );
                    None
                }
            }
        } else {
            None
        };
        let pump_stop = Arc::new(AtomicBool::new(false));
        let pump_submitted = Arc::new(AtomicUsize::new(0));
        let pump_dropped = Arc::new(AtomicUsize::new(0));

        let mut pump = encoder.create_frame_pump();
        let cursor_sample_interval = compute_cursor_sample_interval(target_fps);
        let cursor_sampler_thread = Some(spawn_cursor_sampler(
            start,
            cursor_sampler_stop.clone(),
            cursor_sample_interval,
        ));
        let encoder_shared = Arc::new(Mutex::new(Some(encoder)));

        // For window capture, spawn a pump thread that submits the cached
        // frame at constant FPS.  WGC only delivers frames when the window
        // content changes, which can be <1 fps for a static window.
        if is_window_capture {
            let pump_pool = vram_pool.clone();
            let pump_latest = latest_ready_idx.clone();
            let stop = pump_stop.clone();
            let p_submitted = pump_submitted.clone();
            let p_dropped = pump_dropped.clone();
            let encoder_for_pump = encoder_shared.clone();
            let tick = Duration::from_nanos((frame_interval_100ns * 100) as u64);
            let start_time = start; // Anchor exactly to the global start time
            eprintln!(
                "[FramePump] spawning pump thread: tick={:?} max_pending={}",
                tick, max_pending_frames
            );
            std::thread::spawn(move || {
                unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                }
                eprintln!("[FramePump] pump thread started");
                let mut next_tick = start_time + tick;
                let mut total_submitted: u64 = 0;
                let mut total_dropped: u64 = 0;
                loop {
                    // Check both the explicit pump_stop flag AND the global
                    // SHOULD_STOP.  For window capture, on_frame_arrived may
                    // never fire after stop is requested, so the pump thread
                    // is responsible for driving the shutdown sequence.
                    if stop.load(Ordering::SeqCst) || SHOULD_STOP.load(Ordering::SeqCst) {
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

                        if let Some(enc) = encoder_for_pump.lock().take() {
                            if let Err(e) = enc.finish() {
                                eprintln!("pump thread video encoder finalize error: {}", e);
                            }
                            ENCODING_FINISHED.store(true, Ordering::SeqCst);
                        }
                        break;
                    }

                    let now = Instant::now();
                    if now >= next_tick {
                        let idx = pump_latest.load(Ordering::Acquire);
                        if idx != NO_READY_VRAM_FRAME {
                            while next_tick <= now {
                                let surface = SendDirectX::new(pump_pool[idx].surface.0.clone());
                                let release_counter = Some(pump_pool[idx].in_flight.clone());
                                if pump.submit_surface(surface, max_pending_frames, release_counter)
                                {
                                    p_submitted.fetch_add(1, Ordering::Relaxed);
                                    total_submitted += 1;
                                } else {
                                    p_dropped.fetch_add(1, Ordering::Relaxed);
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

        Ok(Self {
            encoder: encoder_shared,
            target_fps,
            frame_interval_100ns,
            start,
            cursor_sampler_stop,
            cursor_sampler_thread,
            next_submit_timestamp_100ns: Some(0), // Anchor exactly to start time
            last_pending_frames: 0,
            frame_count: 0,
            window_arrivals: 0,
            window_enqueued: 0,
            window_dropped: 0,
            window_paced_skips: 0,
            stats_window_start: Instant::now(),
            enc_w,
            enc_h,
            is_window_capture,
            vram_pool,
            latest_ready_idx,
            write_idx: 0,
            video_processor,
            d3d_device: app_d3d_device,
            d3d_context: app_d3d_context,
            pump_stop,
            pump_submitted,
            pump_dropped,
            max_pending_frames,
            last_ignored_window_frame: None,
            vram_pool_exhausted_logged: false,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        *ACTIVE_CAPTURE_CONTROL.lock() = Some(capture_control.clone());

        if !ENCODER_ACTIVE.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut queue_depth = 0usize;
        let mut dropped_total = 0usize;

        if self.is_window_capture {
            // ── Window capture path ──
            // WGC only delivers frames when the window content changes.
            // We stage the latest frame into a VRAM ring slot; the pump thread
            // submits that surface to the encoder at constant target_fps.
            let frame_w = frame.width();
            let frame_h = frame.height();
            if should_ignore_window_frame(frame_w, frame_h) {
                if self.last_ignored_window_frame != Some((frame_w, frame_h)) {
                    eprintln!(
                        "[FramePump] ignoring implausible window frame {}x{}; keeping last good frame",
                        frame_w, frame_h
                    );
                    self.last_ignored_window_frame = Some((frame_w, frame_h));
                }
            } else {
                self.last_ignored_window_frame = None;
                LAST_CAPTURE_FRAME_WIDTH.store(frame_w as usize, Ordering::Relaxed);
                LAST_CAPTURE_FRAME_HEIGHT.store(frame_h as usize, Ordering::Relaxed);
                let was_empty =
                    self.latest_ready_idx.load(Ordering::Acquire) == NO_READY_VRAM_FRAME;
                match self.stage_frame_in_vram(frame) {
                    Ok(Some(slot)) => {
                        self.vram_pool_exhausted_logged = false;
                        self.latest_ready_idx.store(slot, Ordering::Release);
                        self.window_enqueued = self.window_enqueued.saturating_add(1);
                        if was_empty {
                            eprintln!(
                                "[FramePump] first frame staged in VRAM: frame={}x{} enc={}x{}",
                                frame_w, frame_h, self.enc_w, self.enc_h
                            );
                        }
                    }
                    Ok(None) => {
                        if !self.vram_pool_exhausted_logged {
                            eprintln!(
                                "[FramePump] all staged surfaces still in flight; keeping last good frame until encoder drains"
                            );
                            self.vram_pool_exhausted_logged = true;
                        }
                    }
                    Err(e) => {
                        eprintln!("[FramePump] VRAM stage failed: {}", e);
                    }
                }
            }

            if let Some(encoder) = self.encoder.lock().as_ref() {
                queue_depth = encoder.pending_video_frames();
                dropped_total = encoder.dropped_video_frames();
            }
        } else {
            // ── Display capture path ──
            // WGC delivers frames frequently; submit directly to encoder
            // with pacing/catch-up logic.
            let now_100ns = (self.start.elapsed().as_nanos() / 100) as i64;
            let mut should_submit = false;
            let mut frames_to_submit = 0u32;

            let mut due_100ns = self.next_submit_timestamp_100ns.unwrap_or(0);

            if now_100ns.saturating_add(TIMESTAMP_RESYNC_THRESHOLD_100NS) < due_100ns {
                due_100ns = now_100ns;
            }

            if now_100ns >= due_100ns {
                let due_ticks = ((now_100ns.saturating_sub(due_100ns)) / self.frame_interval_100ns)
                    .saturating_add(1);
                let missed_ticks = due_ticks.saturating_sub(1) as u32;
                frames_to_submit = due_ticks as u32;
                self.window_paced_skips = self.window_paced_skips.saturating_add(missed_ticks);
                self.next_submit_timestamp_100ns = Some(
                    due_100ns.saturating_add(self.frame_interval_100ns.saturating_mul(due_ticks)),
                );
                should_submit = true;
            } else {
                self.window_paced_skips = self.window_paced_skips.saturating_add(1);
                self.next_submit_timestamp_100ns = Some(due_100ns);
            }

            if should_submit {
                let frame_w = frame.width();
                let frame_h = frame.height();
                let staged_mismatch_slot = if frame_w != self.enc_w || frame_h != self.enc_h {
                    match self.stage_frame_in_vram(frame) {
                        Ok(Some(slot)) => Some(slot),
                        Ok(None) => {
                            eprintln!(
                                "Encoder GPU resize fallback skipped: no free staged surface for {}x{} -> {}x{}",
                                frame_w, frame_h, self.enc_w, self.enc_h
                            );
                            None
                        }
                        Err(e) => {
                            eprintln!(
                                "Encoder GPU resize fallback error ({}x{} -> {}x{}): {}",
                                frame_w, frame_h, self.enc_w, self.enc_h, e
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                let mut encoder_guard = self.encoder.lock();
                if let Some(encoder) = encoder_guard.as_mut() {
                    let mut remaining = frames_to_submit.max(1);
                    let mut submitted = 0u32;
                    while remaining > 0 {
                        if submitted >= MAX_CATCHUP_SUBMITS_PER_CALLBACK {
                            encoder.skip_video_frames(remaining);
                            self.window_dropped = self.window_dropped.saturating_add(remaining);
                            break;
                        }

                        if frame_w == self.enc_w && frame_h == self.enc_h {
                            match encoder.send_frame_nonblocking(frame, ENCODER_MAX_PENDING_FRAMES)
                            {
                                Ok(true) => {
                                    self.window_enqueued = self.window_enqueued.saturating_add(1);
                                    submitted = submitted.saturating_add(1);
                                    remaining -= 1;
                                }
                                Ok(false) => {
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Encoder error: {}", e);
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                            }
                        } else {
                            let Some(slot) = staged_mismatch_slot else {
                                encoder.skip_video_frames(remaining);
                                self.window_dropped = self.window_dropped.saturating_add(remaining);
                                break;
                            };

                            let surface = SendDirectX::new(self.vram_pool[slot].surface.0.clone());
                            match encoder.send_directx_surface_nonblocking(
                                surface,
                                self.max_pending_frames,
                                Some(self.vram_pool[slot].in_flight.clone()),
                            ) {
                                Ok(true) => {
                                    self.window_enqueued = self.window_enqueued.saturating_add(1);
                                    submitted = submitted.saturating_add(1);
                                    remaining -= 1;
                                }
                                Ok(false) => {
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                                Err(e) => {
                                    eprintln!("Encoder GPU resize submit error: {}", e);
                                    encoder.skip_video_frames(remaining);
                                    self.window_dropped =
                                        self.window_dropped.saturating_add(remaining);
                                    break;
                                }
                            }
                        }
                    }
                    queue_depth = encoder.pending_video_frames();
                    dropped_total = encoder.dropped_video_frames();
                }
            } else if let Some(encoder) = self.encoder.lock().as_ref() {
                queue_depth = encoder.pending_video_frames();
                dropped_total = encoder.dropped_video_frames();
            }
        }

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

        if SHOULD_STOP.load(Ordering::SeqCst) {
            self.shutdown_and_finalize();
            capture_control.stop();
        }

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.shutdown_and_finalize();
        Ok(())
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
