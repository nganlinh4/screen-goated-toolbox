// --- RECORDING LIFECYCLE ---
// Start and stop recording handler logic: capture initialization,
// encoding wait, and result construction.

use super::super::engine::{
    ACTIVE_CAPTURE_CONTROL, AUDIO_ENCODING_FINISHED, CAPTURE_ERROR, CaptureHandler, ENCODER_ACTIVE,
    ENCODING_FINISHED, IS_RECORDING, LAST_CAPTURE_FRAME_HEIGHT, LAST_CAPTURE_FRAME_WIDTH,
    MIC_AUDIO_ENCODING_FINISHED, MIC_AUDIO_PATH, MIC_AUDIO_START_OFFSET_MS, MOUSE_POSITIONS,
    SHOULD_STOP, VIDEO_PATH, WEBCAM_ENCODING_FINISHED, WEBCAM_VIDEO_PATH,
    WEBCAM_VIDEO_START_OFFSET_MS,
};
use super::super::{SERVER_PORT, input_capture};
use super::media_server::start_global_media_server;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
use windows_capture::capture::GraphicsCaptureApiHandler;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings, SettingsOptions,
};

pub(super) fn handle_start_recording(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    IS_RECORDING.store(true, std::sync::atomic::Ordering::SeqCst);
    let target_type = args["targetType"].as_str().unwrap_or("monitor");
    let target_id = args["targetId"]
        .as_str()
        .or_else(|| args["monitorId"].as_str())
        .unwrap_or("0");
    let include_cursor = args["includeCursor"].as_bool().unwrap_or(false);
    let device_audio_enabled = args["deviceAudioEnabled"].as_bool().unwrap_or(true);
    let device_audio_mode = match args["deviceAudioMode"].as_str() {
        Some("app") if device_audio_enabled => "app",
        _ => "all",
    };
    let device_audio_app_pid = args["deviceAudioAppPid"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|_| device_audio_mode == "app");
    let mic_enabled = args["micEnabled"].as_bool().unwrap_or(false);
    let webcam_enabled = args["webcamEnabled"].as_bool().unwrap_or(true);
    let cursor_setting = if include_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    };

    SHOULD_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    super::super::engine::reset_cursor_detection_state();
    MOUSE_POSITIONS.lock().clear();
    LAST_CAPTURE_FRAME_WIDTH.store(0, std::sync::atomic::Ordering::Relaxed);
    LAST_CAPTURE_FRAME_HEIGHT.store(0, std::sync::atomic::Ordering::Relaxed);
    ACTIVE_CAPTURE_CONTROL.lock().take();
    super::super::engine::EXTERNAL_CAPTURE_CONTROL.lock().take();
    *CAPTURE_ERROR.lock() = None;

    let fps: Option<u32> = args["fps"].as_u64().map(|v| v as u32);
    let flag_str = serde_json::to_string(&serde_json::json!({
        "target_type": target_type,
        "target_id": target_id,
        "fps": fps,
        "device_audio_enabled": device_audio_enabled,
        "device_audio_mode": device_audio_mode,
        "device_audio_app_pid": device_audio_app_pid,
        "mic_enabled": mic_enabled,
        "webcam_enabled": webcam_enabled,
    }))
    .unwrap();

    eprintln!(
        "[CaptureBackend] start_recording: target_type={:?}, target_id={:?}, device_audio_enabled={}, device_audio_mode={}, device_audio_app_pid={:?}, mic_enabled={}, webcam_enabled={}",
        target_type,
        target_id,
        device_audio_enabled,
        device_audio_mode,
        device_audio_app_pid,
        mic_enabled,
        webcam_enabled
    );

    // Request 1ms timer resolution so thread::sleep(1ms) actually sleeps ~1ms
    // instead of the default ~15.6ms Windows scheduler quantum.
    unsafe {
        timeBeginPeriod(1);
    }

    if target_type == "window" {
        let hwnd_val = target_id.parse::<usize>().unwrap_or(0);
        let hwnd = HWND(hwnd_val as *mut _);

        // Log the window title for diagnostics.
        let mut title_buf = [0u16; 256];
        let title_len = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut title_buf)
        };
        let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);
        eprintln!(
            "[CaptureBackend] Window capture: hwnd=0x{:X}, title={:?}, IsWindow={}",
            hwnd_val,
            title,
            unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() }
        );

        if hwnd_val == 0
            || !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() }
        {
            IS_RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(format!("Invalid window handle: 0x{:X}", hwnd_val));
        }

        let window =
            windows_capture::window::Window::from_raw_hwnd(hwnd_val as *mut std::ffi::c_void);

        super::super::engine::TARGET_HWND.store(hwnd_val, std::sync::atomic::Ordering::Relaxed);

        unsafe {
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
            super::super::engine::MONITOR_X = rect.left;
            super::super::engine::MONITOR_Y = rect.top;
        }

        let update_interval = if let Some(f) = fps {
            let target_micros = 1_000_000 / f.max(1);
            MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_micros(
                (target_micros / 2) as u64,
            ))
        } else {
            MinimumUpdateIntervalSettings::Default
        };

        let settings = Settings::new(
            window,
            SettingsOptions {
                cursor_capture_settings: cursor_setting,
                draw_border_settings: DrawBorderSettings::WithoutBorder,
                secondary_window_settings: SecondaryWindowSettings::Default,
                minimum_update_interval_settings: update_interval,
                dirty_region_settings: DirtyRegionSettings::Default,
                color_format: ColorFormat::Bgra8,
                flags: flag_str,
            },
        );

        match CaptureHandler::start_free_threaded(settings) {
            Ok(control) => {
                *super::super::engine::EXTERNAL_CAPTURE_CONTROL.lock() = Some(control);
            }
            Err(e) => {
                let msg = format!("Window capture failed: {}", e);
                eprintln!("[CaptureBackend] {}", msg);
                *CAPTURE_ERROR.lock() = Some(msg.clone());
                IS_RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(msg);
            }
        }

        // Show a distinct blue border around the captured window.
        super::super::capture_border::show_capture_border(hwnd);
    } else {
        super::super::engine::TARGET_HWND.store(0, std::sync::atomic::Ordering::Relaxed);
        let monitor_index = target_id.parse::<usize>().unwrap_or(0);
        let monitor = Monitor::from_index(monitor_index + 1).map_err(|e| e.to_string())?;

        unsafe {
            let mut monitors: Vec<windows::Win32::Graphics::Gdi::HMONITOR> = Vec::new();
            let _ = windows::Win32::Graphics::Gdi::EnumDisplayMonitors(
                None,
                None,
                Some(super::super::engine::monitor_enum_proc),
                LPARAM(&mut monitors as *mut _ as isize),
            );
            if let Some(&hmonitor) = monitors.get(monitor_index) {
                let mut info: windows::Win32::Graphics::Gdi::MONITORINFOEXW = std::mem::zeroed();
                info.monitorInfo.cbSize =
                    std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFOEXW>() as u32;
                if windows::Win32::Graphics::Gdi::GetMonitorInfoW(
                    hmonitor,
                    &mut info.monitorInfo as *mut _,
                )
                .as_bool()
                {
                    super::super::engine::MONITOR_X = info.monitorInfo.rcMonitor.left;
                    super::super::engine::MONITOR_Y = info.monitorInfo.rcMonitor.top;
                }
            }
        }

        let update_interval = if let Some(f) = fps {
            let target_micros = 1_000_000 / f.max(1);
            MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_micros(
                (target_micros / 2) as u64,
            ))
        } else {
            MinimumUpdateIntervalSettings::Default
        };

        let settings = Settings::new(
            monitor,
            SettingsOptions {
                cursor_capture_settings: cursor_setting,
                draw_border_settings: DrawBorderSettings::Default,
                secondary_window_settings: SecondaryWindowSettings::Include,
                minimum_update_interval_settings: update_interval,
                dirty_region_settings: DirtyRegionSettings::Default,
                color_format: ColorFormat::Bgra8,
                flags: flag_str,
            },
        );

        match CaptureHandler::start_free_threaded(settings) {
            Ok(control) => {
                *super::super::engine::EXTERNAL_CAPTURE_CONTROL.lock() = Some(control);
            }
            Err(e) => {
                let msg = format!("Display capture failed: {}", e);
                eprintln!("[CaptureBackend] {}", msg);
                *CAPTURE_ERROR.lock() = Some(msg.clone());
                IS_RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(msg);
            }
        }
    }

    if let Err(err) = input_capture::start_capture() {
        crate::log_info!("Input capture start failed: {}", err);
    }

    println!(
        "[CaptureBackend] selected=wgc reason=single_active_backend targetType={}",
        target_type
    );
    println!(
        "[CaptureBackend] cursor_capture_mode={}",
        if include_cursor {
            "with_cursor"
        } else {
            "without_cursor"
        }
    );

    Ok(serde_json::Value::Null)
}

pub(super) fn handle_stop_recording() -> Result<serde_json::Value, String> {
    SHOULD_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
    super::super::engine::TARGET_HWND.store(0, std::sync::atomic::Ordering::SeqCst);
    super::super::capture_border::hide_capture_border();

    // Restore default timer resolution (matching the timeBeginPeriod in start_recording).
    unsafe {
        timeEndPeriod(1);
    }
    if let Some(control) = ACTIVE_CAPTURE_CONTROL.lock().take() {
        control.stop();
    }
    let raw_input_events = input_capture::stop_capture_and_drain();

    // Check if capture failed to start (error stored by the capture thread).
    // Give the capture thread a brief moment to report failure.
    std::thread::sleep(std::time::Duration::from_millis(200));
    if let Some(err_msg) = CAPTURE_ERROR.lock().take() {
        // Clean up all recording state so nothing keeps running.
        IS_RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);
        ENCODER_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        super::super::engine::SHOULD_STOP_AUDIO.store(true, std::sync::atomic::Ordering::SeqCst);
        ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
        AUDIO_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
        MIC_AUDIO_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
        WEBCAM_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
        return Err(err_msg);
    }

    // Wait for encoding to finish.
    //
    // Display capture: on_frame_arrived fires at ~50fps, quickly detects
    //   SHOULD_STOP, and calls shutdown_and_finalize → encoder.finish().
    //
    // Window capture: the pump thread detects SHOULD_STOP, waits for
    //   audio to flush, sends EOF to the MF transcode, then on_frame_arrived
    //   (which still fires occasionally at 0.8-18fps from WGC) triggers
    //   shutdown_and_finalize → encoder.finish() (fast: transcode already
    //   completed from pump's EOF signals).
    let start = std::time::Instant::now();
    while (!ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
        || !AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
        || !MIC_AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
        || !WEBCAM_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst))
        && start.elapsed().as_secs() < 10
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let encoding_done = ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
        && AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
        && MIC_AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
        && WEBCAM_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst);

    if !encoding_done {
        eprintln!(
            "[CaptureBackend] Encoding did not finish within timeout. \
             Stopping capture thread and cleaning up."
        );

        // Force-stop the capture thread so on_closed → shutdown_and_finalize
        // runs.  This is the fallback if on_frame_arrived never fired.
        if let Some(control) = super::super::engine::EXTERNAL_CAPTURE_CONTROL.lock().take() {
            let _ = control.stop();
        }

        // Give shutdown_and_finalize's spawned thread a moment to set
        // ENCODING_FINISHED after the capture thread is stopped.
        let retry_start = std::time::Instant::now();
        while (!ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
            || !AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
            || !MIC_AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
            || !WEBCAM_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst))
            && retry_start.elapsed().as_secs() < 5
        {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let retry_done = ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
            && AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
            && MIC_AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
            && WEBCAM_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst);

        if !retry_done {
            IS_RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);
            ENCODER_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
            super::super::engine::SHOULD_STOP_AUDIO
                .store(true, std::sync::atomic::Ordering::SeqCst);
            ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
            AUDIO_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
            MIC_AUDIO_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
            WEBCAM_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);

            if let Some(ref path) = *VIDEO_PATH.lock().unwrap() {
                let _ = std::fs::remove_file(path);
            }
            if let Some(ref path) = *MIC_AUDIO_PATH.lock().unwrap() {
                let _ = std::fs::remove_file(path);
            }
            if let Some(ref path) = *WEBCAM_VIDEO_PATH.lock().unwrap() {
                let _ = std::fs::remove_file(path);
            }

            return Err("Recording failed: encoding did not complete in time. \
                 Please try again."
                .to_string());
        }
    }

    // Clean up the capture thread now that encoding is done.
    if let Some(control) = super::super::engine::EXTERNAL_CAPTURE_CONTROL.lock().take() {
        let _ = control.stop();
    }

    let video_path = VIDEO_PATH.lock().unwrap().clone().ok_or("No video path")?;
    let video_file_path = video_path.clone();
    let mic_audio_path = MIC_AUDIO_PATH.lock().unwrap().clone();
    let webcam_video_path = WEBCAM_VIDEO_PATH.lock().unwrap().clone();
    let mic_audio_offset_sec =
        match MIC_AUDIO_START_OFFSET_MS.load(std::sync::atomic::Ordering::SeqCst) {
            u64::MAX => 0.0,
            value => value as f64 / 1000.0,
        };
    let webcam_video_offset_sec =
        match WEBCAM_VIDEO_START_OFFSET_MS.load(std::sync::atomic::Ordering::SeqCst) {
            u64::MAX => 0.0,
            value => value as f64 / 1000.0,
        };
    let last_recording_fps = *super::super::engine::LAST_RECORDING_FPS.lock().unwrap();

    let mut port = SERVER_PORT.load(std::sync::atomic::Ordering::SeqCst);
    if port == 0 {
        port = start_global_media_server().unwrap_or(0);
    }

    let mouse_positions = MOUSE_POSITIONS.lock().drain(..).collect::<Vec<_>>();

    let encoded_path = urlencoding::encode(&video_path);
    let video_url = format!("http://localhost:{}/?path={}", port, encoded_path);
    let device_audio_url = format!("http://localhost:{}/?path={}", port, encoded_path);
    let mic_audio_url = mic_audio_path
        .as_ref()
        .filter(|path| std::path::Path::new(path).exists())
        .map(|path| {
            let encoded = urlencoding::encode(path);
            format!("http://localhost:{}/?path={}", port, encoded)
        });
    let webcam_video_url = webcam_video_path
        .as_ref()
        .filter(|path| {
            std::fs::metadata(path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        })
        .map(|path| {
            let encoded = urlencoding::encode(path);
            format!("http://localhost:{}/?path={}", port, encoded)
        });
    IS_RECORDING.store(false, std::sync::atomic::Ordering::SeqCst);

    Ok(serde_json::json!({
        "videoUrl": video_url,
        "deviceAudioUrl": device_audio_url,
        "micAudioUrl": mic_audio_url.unwrap_or_default(),
        "webcamVideoUrl": webcam_video_url.unwrap_or_default(),
        "mouseData": mouse_positions,
        "deviceAudioPath": video_file_path,
        "micAudioPath": mic_audio_path.unwrap_or_default(),
        "webcamVideoPath": webcam_video_path.as_ref()
            .filter(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))
            .cloned()
            .unwrap_or_default(),
        "micAudioOffsetSec": mic_audio_offset_sec,
        "webcamVideoOffsetSec": webcam_video_offset_sec,
        "videoFilePath": video_file_path,
        "inputEvents": raw_input_events,
        "capturedFps": last_recording_fps
    }))
}
