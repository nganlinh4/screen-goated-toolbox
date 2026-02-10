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
    encoder::{AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder},
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
}

pub static VIDEO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static AUDIO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static mut MONITOR_X: i32 = 0;
pub static mut MONITOR_Y: i32 = 0;

const DEFAULT_GRAB_SIGNATURE: &str = "hot(13,13)|mask(32x32)|color(32x32)|mono(0)";

pub struct CaptureHandler {
    encoder: Option<VideoEncoder>,
    start: Instant,
    last_mouse_capture: Instant,
    frame_count: u32,
}

fn get_cursor_type(is_clicked: bool) -> String {
    unsafe {
        let mut cursor_info: CURSORINFO = std::mem::zeroed();
        cursor_info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;

        if GetCursorInfo(&mut cursor_info).is_ok() && cursor_info.flags.0 != 0 {
            let current_handle = cursor_info.hCursor.0;
            let current_handle_key = current_handle as isize;

            let arrow = LoadCursorW(None, IDC_ARROW).unwrap().0;
            let ibeam = LoadCursorW(None, IDC_IBEAM).unwrap().0;
            let wait = LoadCursorW(None, IDC_WAIT).unwrap().0;
            let appstarting = LoadCursorW(None, IDC_APPSTARTING).unwrap().0;
            let cross = LoadCursorW(None, IDC_CROSS).unwrap().0;
            let hand = LoadCursorW(None, IDC_HAND).unwrap().0;
            let size_all = LoadCursorW(None, IDC_SIZEALL).unwrap().0;
            let size_ns = LoadCursorW(None, IDC_SIZENS).unwrap().0;
            let size_we = LoadCursorW(None, IDC_SIZEWE).unwrap().0;
            let size_nwse = LoadCursorW(None, IDC_SIZENWSE).unwrap().0;
            let size_nesw = LoadCursorW(None, IDC_SIZENESW).unwrap().0;

            let signature = cursor_signature(cursor_info.hCursor).unwrap_or_else(|| "n/a".to_string());
            let cursor_type = if current_handle == arrow {
                "default".to_string()
            } else if current_handle == ibeam {
                "text".to_string()
            } else if current_handle == wait {
                "wait".to_string()
            } else if current_handle == appstarting {
                "appstarting".to_string()
            } else if current_handle == cross {
                "crosshair".to_string()
            } else if current_handle == size_all {
                "move".to_string()
            } else if current_handle == size_ns {
                "resize_ns".to_string()
            } else if current_handle == size_we {
                "resize_we".to_string()
            } else if current_handle == size_nwse {
                "resize_nwse".to_string()
            } else if current_handle == size_nesw {
                "resize_nesw".to_string()
            } else if current_handle == hand {
                "pointer".to_string()
            } else if CUSTOM_GRAB_SIGNATURES.lock().contains(&signature) {
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
            };

            // Debug logging: emit only when cursor handle/type/click-state changes.
            let mut last = LAST_CURSOR_DEBUG.lock();
            let changed = match &*last {
                Some((h, t, c, s)) => {
                    *h != current_handle_key || t != &cursor_type || *c != is_clicked || s != &signature
                }
                None => true,
            };
            if changed {
                println!(
                    "[CursorDetect] handle=0x{:X} type={} clicked={} sig={}",
                    current_handle_key as usize,
                    cursor_type,
                    is_clicked,
                    signature
                );
                *last = Some((current_handle_key, cursor_type.clone(), is_clicked, signature));
            }

            cursor_type
        } else {
            "default".to_string()
        }
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

        // DYNAMIC BITRATE CALCULATION
        // Old Value: 15_000_000 (15 Mbps) -> Resulted in blur on High-DPI/Text
        // New Value: Based on resolution. Target 0.35 bits per pixel for high quality capture.
        // 1920x1080 @ 60fps = ~43 Mbps
        // 2560x1440 @ 60fps = ~79 Mbps
        // 3840x2160 @ 60fps = ~179 Mbps
        let pixel_count = width as u64 * height as u64;
        let target_bitrate = (pixel_count as f64 * 60.0 * 0.35) as u32;

        // Cap bitrate at 150Mbps to avoid massive files for 4K while maintaining very high quality
        let final_bitrate = target_bitrate.clamp(10_000_000, 150_000_000);

        let video_settings = VideoSettingsBuilder::new(width, height)
            .frame_rate(60)
            .bitrate(final_bitrate);

        println!(
            "Initializing VideoEncoder: {}x{} @ 60fps, Bitrate: {} Mbps, Monitor Index: {}",
            width,
            height,
            final_bitrate / 1_000_000,
            monitor_index
        );

        let encoder = VideoEncoder::new(
            video_settings,
            AudioSettingsBuilder::default().disabled(true),
            ContainerSettingsBuilder::default(),
            &video_path,
        )?;

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
            start: Instant::now(),
            last_mouse_capture: Instant::now(),
            frame_count: 0,
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

        if let Err(e) = self.encoder.as_mut().unwrap().send_frame(frame) {
            eprintln!("Encoder error: {}", e);
        }

        if self.frame_count % 60 == 0 {
            // Debug log every ~1 second
            println!("Captured frame {}", self.frame_count);
        }
        self.frame_count += 1;

        if self.last_mouse_capture.elapsed().as_millis() >= 16 {
            unsafe {
                let mut point = POINT::default();
                if GetCursorPos(&mut point).is_ok() {
                    // Record actual held state - cursor should stay squished while held
                    let is_clicked = IS_MOUSE_CLICKED.load(Ordering::SeqCst);
                    let cursor_type = get_cursor_type(is_clicked);

                    let mouse_pos = MousePosition {
                        x: point.x - MONITOR_X,
                        y: point.y - MONITOR_Y,
                        timestamp: self.start.elapsed().as_secs_f64(),
                        is_clicked,
                        cursor_type,
                    };

                    MOUSE_POSITIONS.lock().push_back(mouse_pos);
                }
            }
            self.last_mouse_capture = Instant::now();
        }

        if SHOULD_STOP.load(Ordering::SeqCst) {
            ENCODER_ACTIVE.store(false, Ordering::SeqCst);
            SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
            if let Some(encoder) = self.encoder.take() {
                std::thread::spawn(move || {
                    let _ = encoder.finish();
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
