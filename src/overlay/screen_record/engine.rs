use crate::overlay::screen_record::audio_engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs;
use std::mem::zeroed;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, GetObjectW, BITMAP, HDC, HMONITOR,
    MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorInfo, GetCursorPos, GetIconInfo, LoadCursorW, CURSORINFO, ICONINFO, IDC_APPSTARTING,
    IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE,
    IDC_SIZEWE, IDC_WAIT,
};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    },
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
    pub timestamp: f64,
    pub is_clicked: bool,
    pub cursor_type: String,
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
}

lazy_static::lazy_static! {
    pub static ref MOUSE_POSITIONS: Mutex<VecDeque<MousePosition>> = Mutex::new(VecDeque::new());
    pub static ref IS_RECORDING: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref SHOULD_STOP: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref SHOULD_STOP_AUDIO: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref ENCODING_FINISHED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_ENCODING_FINISHED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref ENCODER_ACTIVE: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref IS_MOUSE_CLICKED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    // Track if we already captured the click event (to only record one frame as clicked)
    pub static ref CLICK_CAPTURED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    // Last emitted cursor debug record to avoid spamming logs every frame.
    static ref LAST_CURSOR_DEBUG: Mutex<Option<(isize, String, bool, String)>> = Mutex::new(None);
    // Learned non-system custom cursor signatures that represent grab/openhand cursors.
    // Learned only when unknown cursor appears while clicked=true.
    static ref CUSTOM_GRAB_SIGNATURES: Mutex<HashSet<String>> = {
        Mutex::new(load_grab_signatures())
    };
    // Resolve system cursor handles once; avoids repeated LoadCursorW calls per sample.
    static ref SYSTEM_CURSOR_HANDLES: SystemCursorHandles = load_system_cursor_handles();
    // Set SCREEN_RECORD_CURSOR_DEBUG=1 to enable verbose cursor classification logs.
    static ref CURSOR_DEBUG_ENABLED: bool = {
        std::env::var("SCREEN_RECORD_CURSOR_DEBUG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    };
}

pub static VIDEO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static AUDIO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static mut MONITOR_X: i32 = 0;
pub static mut MONITOR_Y: i32 = 0;

const DEFAULT_GRAB_SIGNATURE: &str = "hot(13,13)|mask(32x32)|color(32x32)|mono(0)";
const DEFAULT_TARGET_FPS: u32 = 60;
const ENCODER_MAX_PENDING_FRAMES: usize = 12;
const MAX_CATCHUP_SUBMITS_PER_CALLBACK: u32 = 6;
const TIMESTAMP_RESYNC_THRESHOLD_100NS: i64 = 10_000_000;
const CAPTURE_STATS_WINDOW_SECS: f64 = 1.0;

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

pub struct CaptureHandler {
    encoder: Option<VideoEncoder>,
    target_fps: u32,
    frame_interval_100ns: i64,
    start: Instant,
    last_mouse_capture: Instant,
    next_submit_timestamp_100ns: Option<i64>,
    last_pending_frames: usize,
    frame_count: u64,
    window_arrivals: u32,
    window_enqueued: u32,
    window_dropped: u32,
    window_paced_skips: u32,
    stats_window_start: Instant,
}

fn select_target_fps(monitor_hz: u32) -> u32 {
    // Prefer exact monitor divisors in the 50-60fps export band.
    // Example: 165Hz -> 55fps (exact), which removes recurring pacing drift.
    for candidate in (50..=60).rev() {
        if monitor_hz % candidate == 0 {
            return candidate;
        }
    }

    DEFAULT_TARGET_FPS
}

fn mf_hw_accel_enabled() -> bool {
    match std::env::var("SCREEN_RECORD_MF_HW_ACCEL") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
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
                "default".to_string()
            } else if current_handle_key == handles.ibeam {
                "text".to_string()
            } else if current_handle_key == handles.wait {
                "wait".to_string()
            } else if current_handle_key == handles.appstarting {
                "appstarting".to_string()
            } else if current_handle_key == handles.cross {
                "crosshair".to_string()
            } else if current_handle_key == handles.size_all {
                "move".to_string()
            } else if current_handle_key == handles.size_ns {
                "resize_ns".to_string()
            } else if current_handle_key == handles.size_we {
                "resize_we".to_string()
            } else if current_handle_key == handles.size_nwse {
                "resize_nwse".to_string()
            } else if current_handle_key == handles.size_nesw {
                "resize_nesw".to_string()
            } else if current_handle_key == handles.hand {
                "pointer".to_string()
            } else {
                signature =
                    cursor_signature(cursor_info.hCursor).unwrap_or_else(|| "n/a".to_string());
                if CUSTOM_GRAB_SIGNATURES.lock().contains(&signature) {
                    "grab".to_string()
                } else if is_clicked {
                    let should_persist = {
                        let mut set = CUSTOM_GRAB_SIGNATURES.lock();
                        set.insert(signature.clone())
                    };
                    if should_persist {
                        println!("[CursorDetect] learn-grab-signature {}", signature);
                        persist_grab_signatures();
                    }
                    "grab".to_string()
                } else {
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
    unsafe { LoadCursorW(None, cursor_id).map(|cursor| cursor.0 as isize).unwrap_or_default() }
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

        if !icon_info.hbmMask.0.is_null() {
            let _ = DeleteObject(icon_info.hbmMask.into());
        }
        if !icon_info.hbmColor.0.is_null() {
            let _ = DeleteObject(icon_info.hbmColor.into());
        }

        Some(format!(
            "hot({},{})|mask({}x{})|color({}x{})|mono({})",
            icon_info.xHotspot,
            icon_info.yHotspot,
            mask_bm.bmWidth,
            mask_bm.bmHeight,
            color_bm.bmWidth,
            color_bm.bmHeight,
            if icon_info.hbmColor.0.is_null() { 1 } else { 0 }
        ))
    }
}

fn sample_mouse_position(start: Instant, last_mouse_capture: &mut Instant) {
    if last_mouse_capture.elapsed().as_millis() < 16 {
        return;
    }

    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let is_clicked = IS_MOUSE_CLICKED.load(Ordering::SeqCst);
            let cursor_type = get_cursor_type(is_clicked);

            let mouse_pos = MousePosition {
                x: point.x - MONITOR_X,
                y: point.y - MONITOR_Y,
                timestamp: start.elapsed().as_secs_f64(),
                is_clicked,
                cursor_type,
            };
            MOUSE_POSITIONS.lock().push_back(mouse_pos);
        }
    }
    *last_mouse_capture = Instant::now();
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = String;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let monitor_index = ctx.flags.parse::<usize>().unwrap_or(0);

        let monitor = Monitor::from_index(monitor_index + 1)?;
        let mut width = monitor.width()?;
        let mut height = monitor.height()?;

        // Ensure even dimensions for encoding
        if width % 2 != 0 {
            width -= 1;
        }
        if height % 2 != 0 {
            height -= 1;
        }

        let app_data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::env::temp_dir())
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

        let audio_path = app_data_dir.join(format!(
            "recording_{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));

        *VIDEO_PATH.lock().unwrap() = Some(video_path.to_string_lossy().to_string());
        *AUDIO_PATH.lock().unwrap() = Some(audio_path.to_string_lossy().to_string());

        let monitor_hz = monitor.refresh_rate().unwrap_or(DEFAULT_TARGET_FPS).max(1);
        let target_fps = select_target_fps(monitor_hz);
        let frame_interval_100ns = 10_000_000 / target_fps as i64;

        // DYNAMIC BITRATE CALCULATION
        // Prior 0.35 bpp target could trigger intermittent MediaFoundation HW encoder
        // backpressure during heavy gameplay. Use a more stable 0.22 bpp target.
        // 1920x1080 @ 60fps = ~27 Mbps
        // 2560x1440 @ 60fps = ~48 Mbps
        // 3840x2160 @ 60fps = ~109 Mbps
        let pixel_count = width as u64 * height as u64;
        let target_bitrate = (pixel_count as f64 * target_fps as f64 * 0.22) as u32;

        // Keep a quality floor while capping peak encoder pressure.
        let final_bitrate = target_bitrate.clamp(8_000_000, 80_000_000);

        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(width, height)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(final_bitrate)
                .frame_rate(target_fps),
            AudioSettingsBuilder::new().disabled(true),
            ContainerSettingsBuilder::new(),
            &video_path,
        )?;
        println!(
            "Initializing VideoEncoder: {}x{} @ {}fps (monitor={}Hz), Codec: H264 (MediaFoundation {}), Bitrate: {} Mbps, Monitor Index: {}",
            width,
            height,
            target_fps,
            monitor_hz,
            if mf_hw_accel_enabled() { "HW" } else { "SW" },
            final_bitrate / 1_000_000,
            monitor_index
        );

        SHOULD_STOP_AUDIO.store(false, Ordering::SeqCst);
        AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
        audio_engine::record_audio(
            audio_path.to_string_lossy().to_string(),
            SHOULD_STOP_AUDIO.clone(),
            AUDIO_ENCODING_FINISHED.clone(),
        );

        ENCODER_ACTIVE.store(true, Ordering::SeqCst);
        ENCODING_FINISHED.store(false, Ordering::SeqCst);

        Ok(Self {
            encoder: Some(encoder),
            target_fps,
            frame_interval_100ns,
            start: Instant::now(),
            last_mouse_capture: Instant::now(),
            next_submit_timestamp_100ns: None,
            last_pending_frames: 0,
            frame_count: 0,
            window_arrivals: 0,
            window_enqueued: 0,
            window_dropped: 0,
            window_paced_skips: 0,
            stats_window_start: Instant::now(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if !ENCODER_ACTIVE.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut queue_depth = 0usize;
        let mut dropped_total = 0usize;
        let now_100ns = (self.start.elapsed().as_nanos() / 100) as i64;
        let mut should_submit = false;
        let mut frames_to_submit = 0u32;

        match self.next_submit_timestamp_100ns {
            Some(mut due_100ns) => {
                if now_100ns.saturating_add(TIMESTAMP_RESYNC_THRESHOLD_100NS) < due_100ns {
                    due_100ns = now_100ns;
                }

                if now_100ns >= due_100ns {
                    let due_ticks =
                        ((now_100ns.saturating_sub(due_100ns)) / self.frame_interval_100ns)
                            .saturating_add(1);
                    let missed_ticks = due_ticks.saturating_sub(1) as u32;
                    frames_to_submit = due_ticks as u32;
                    self.window_paced_skips = self.window_paced_skips.saturating_add(missed_ticks);
                    self.next_submit_timestamp_100ns = Some(
                        due_100ns
                            .saturating_add(self.frame_interval_100ns.saturating_mul(due_ticks)),
                    );
                    should_submit = true;
                } else {
                    self.window_paced_skips = self.window_paced_skips.saturating_add(1);
                    self.next_submit_timestamp_100ns = Some(due_100ns);
                }
            }
            None => {
                self.next_submit_timestamp_100ns =
                    Some(now_100ns.saturating_add(self.frame_interval_100ns));
                frames_to_submit = 1;
                should_submit = true;
            }
        }

        if should_submit {
            if let Some(encoder) = self.encoder.as_mut() {
                // OBS-style catch-up: when we miss ticks, duplicate the latest frame
                // for those ticks instead of jumping timeline forward.
                let mut remaining = frames_to_submit.max(1);
                let mut submitted = 0u32;
                let mut used_blocking_fallback = false;
                while remaining > 0 {
                    if submitted >= MAX_CATCHUP_SUBMITS_PER_CALLBACK {
                        // Avoid long duplicate bursts in one callback after a hitch.
                        // Advance timeline for any unsent tail to keep A/V in sync.
                        encoder.skip_video_frames(remaining);
                        self.window_dropped = self.window_dropped.saturating_add(remaining);
                        break;
                    }

                    match encoder.send_frame_nonblocking(frame, ENCODER_MAX_PENDING_FRAMES) {
                        Ok(true) => {
                            self.window_enqueued = self.window_enqueued.saturating_add(1);
                            submitted = submitted.saturating_add(1);
                            remaining -= 1;
                        }
                        Ok(false) => {
                            // On transient queue-full pressure, wait once for encoder
                            // consumption before dropping the remainder.
                            if !used_blocking_fallback {
                                used_blocking_fallback = true;
                                match encoder.send_frame(frame) {
                                    Ok(()) => {
                                        self.window_enqueued =
                                            self.window_enqueued.saturating_add(1);
                                        submitted = submitted.saturating_add(1);
                                        remaining -= 1;
                                        continue;
                                    }
                                    Err(e) => {
                                        eprintln!("Encoder blocking fallback error: {}", e);
                                    }
                                }
                            }

                            // Sustained queue-full: preserve timeline for unsent tail
                            // so video duration stays aligned with audio duration.
                            encoder.skip_video_frames(remaining);
                            self.window_dropped = self.window_dropped.saturating_add(remaining);
                            break;
                        }
                        Err(e) => {
                            eprintln!("Encoder error: {}", e);
                            encoder.skip_video_frames(remaining);
                            self.window_dropped = self.window_dropped.saturating_add(remaining);
                            break;
                        }
                    }
                }
                queue_depth = encoder.pending_video_frames();
                dropped_total = encoder.dropped_video_frames();
            }
        } else {
            if let Some(encoder) = self.encoder.as_ref() {
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
                .as_ref()
                .map(|encoder| encoder.pending_video_frames())
                .unwrap_or(self.last_pending_frames);
            let encoded_window = (self.last_pending_frames + self.window_enqueued as usize)
                .saturating_sub(pending_now);
            self.last_pending_frames = pending_now;
            let encoded_fps = encoded_window as f64 / elapsed.max(0.001);
            println!(
                "[CaptureStats] backend=monitor capture_fps={:.1} queue_fps={:.1} encode_fps={:.1} target_fps={} paced_skips={} queue_depth={} dropped_window={} dropped_total={}",
                capture_fps,
                queued_fps,
                encoded_fps,
                self.target_fps,
                self.window_paced_skips,
                queue_depth,
                self.window_dropped,
                dropped_total
            );
            self.window_arrivals = 0;
            self.window_enqueued = 0;
            self.window_dropped = 0;
            self.window_paced_skips = 0;
            self.stats_window_start = Instant::now();
        }

        sample_mouse_position(self.start, &mut self.last_mouse_capture);

        if SHOULD_STOP.load(Ordering::SeqCst) {
            ENCODER_ACTIVE.store(false, Ordering::SeqCst);
            SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
            if let Some(encoder) = self.encoder.take() {
                std::thread::spawn(move || {
                    if let Err(error) = encoder.finish() {
                        eprintln!("video encoder finalize error: {}", error);
                    }
                    ENCODING_FINISHED.store(true, Ordering::SeqCst);
                });
            }
            capture_control.stop();
        }

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
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
                monitor_infos.push(MonitorInfo {
                    id: index.to_string(),
                    name: format!("Display {}", index + 1),
                    x: rect.left,
                    y: rect.top,
                    width: (rect.right - rect.left) as u32,
                    height: (rect.bottom - rect.top) as u32,
                    is_primary: info.monitorInfo.dwFlags & 1 == 1,
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
    let monitors = &mut *(lparam.0 as *mut Vec<HMONITOR>);
    monitors.push(hmonitor);
    true.into()
}
