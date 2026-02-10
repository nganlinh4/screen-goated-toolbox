// --- SCREEN RECORD IPC ---
// IPC command handling for screen recording WebView.

use super::engine::{
    get_monitors, CaptureHandler, AUDIO_ENCODING_FINISHED, AUDIO_PATH, ENCODING_FINISHED,
    MOUSE_POSITIONS, SHOULD_STOP, VIDEO_PATH,
};
use super::ffmpeg::{
    get_ffmpeg_path, get_ffprobe_path, start_ffmpeg_installation, FfmpegInstallStatus,
    FFMPEG_INSTALL_STATUS,
};
use super::keyviz;
use super::native_export;
use super::{SERVER_PORT, SR_HWND};
use crate::config::Hotkey;
use crate::APP;
use regex::Regex;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::thread;
use tiny_http::{Response, Server, StatusCode};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_capture::capture::GraphicsCaptureApiHandler;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

const WM_RELOAD_HOTKEYS: u32 = WM_USER + 101;
const WM_UNREGISTER_HOTKEYS: u32 = WM_USER + 103;
const WM_REGISTER_HOTKEYS: u32 = WM_USER + 104;

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

pub fn handle_ipc_command(
    cmd: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match cmd.as_str() {
        "check_ffmpeg_status" => {
            let ffmpeg_path = get_ffmpeg_path();
            let ffprobe_path = get_ffprobe_path();
            let ffmpeg_missing = !ffmpeg_path.exists();
            let ffprobe_missing = !ffprobe_path.exists();
            Ok(serde_json::json!({
                "ffmpegMissing": ffmpeg_missing,
                "ffprobeMissing": ffprobe_missing
            }))
        }
        "start_ffmpeg_install" => {
            start_ffmpeg_installation();
            Ok(serde_json::Value::Null)
        }
        "get_ffmpeg_install_progress" => {
            let status = FFMPEG_INSTALL_STATUS.lock().unwrap().clone();
            Ok(serde_json::to_value(&status).unwrap())
        }
        "cancel_ffmpeg_install" => {
            *FFMPEG_INSTALL_STATUS.lock().unwrap() = FfmpegInstallStatus::Cancelled;
            Ok(serde_json::Value::Null)
        }
        "start_export_server" => native_export::start_native_export(args),
        "cancel_export" => {
            println!("[Cancel] IPC cancel_export received");
            native_export::cancel_export();
            println!("[Cancel] cancel_export() returned");
            Ok(serde_json::Value::Null)
        }
        "get_default_export_dir" => Ok(serde_json::json!(native_export::get_default_export_dir())),
        "pick_export_folder" => {
            let initial_dir = args["initialDir"].as_str().map(|s| s.to_string());
            let selected = native_export::pick_export_folder(initial_dir)?;
            Ok(match selected {
                Some(path) => serde_json::json!(path),
                None => serde_json::Value::Null,
            })
        }
        "apply_cursor_svg_adjustment" => {
            let src = args["src"].as_str().ok_or("Missing src")?;
            let scale = args["scale"].as_f64().ok_or("Missing scale")? as f32;
            let offset_x = args["offsetX"].as_f64().ok_or("Missing offsetX")? as f32;
            let offset_y = args["offsetY"].as_f64().ok_or("Missing offsetY")? as f32;

            let files = apply_cursor_svg_adjustment(src, scale, offset_x, offset_y)?;
            Ok(serde_json::json!({
                "ok": true,
                "filesUpdated": files,
            }))
        }
        "get_monitors" => {
            let monitors = get_monitors();
            Ok(serde_json::to_value(monitors).unwrap())
        }
        "install_keyviz" => {
            std::thread::spawn(|| {
                if let Err(e) = keyviz::install_keyviz() {
                    crate::log_info!("Keyviz install failed: {}", e);
                }
            });
            Ok(serde_json::Value::Null)
        }
        "set_keyviz_enabled" => {
            let enabled = args["enabled"].as_bool().unwrap_or(false);
            keyviz::set_enabled(enabled);
            Ok(serde_json::Value::Null)
        }
        "get_keyviz_status" => Ok(serde_json::json!({
            "installed": keyviz::is_installed(),
            "enabled": keyviz::is_enabled()
        })),
        "start_recording" => {
            let monitor_id = args["monitorId"].as_str().unwrap_or("0");
            let monitor_index = monitor_id.parse::<usize>().unwrap_or(0);

            SHOULD_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
            super::engine::IS_MOUSE_CLICKED.store(false, std::sync::atomic::Ordering::SeqCst);
            super::engine::CLICK_CAPTURED.store(false, std::sync::atomic::Ordering::SeqCst);
            MOUSE_POSITIONS.lock().clear();

            let monitor = Monitor::from_index(monitor_index + 1).map_err(|e| e.to_string())?;

            unsafe {
                let mut monitors: Vec<windows::Win32::Graphics::Gdi::HMONITOR> = Vec::new();
                let _ = windows::Win32::Graphics::Gdi::EnumDisplayMonitors(
                    None,
                    None,
                    Some(super::engine::monitor_enum_proc),
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
                        super::engine::MONITOR_X = info.monitorInfo.rcMonitor.left;
                        super::engine::MONITOR_Y = info.monitorInfo.rcMonitor.top;
                    }
                }
            }

            let settings = Settings::new(
                monitor,
                CursorCaptureSettings::WithoutCursor,
                DrawBorderSettings::Default,
                SecondaryWindowSettings::Include,
                MinimumUpdateIntervalSettings::Default,
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                monitor_id.to_string(),
            );

            let _ = keyviz::start();

            std::thread::spawn(move || {
                let _ = CaptureHandler::start_free_threaded(settings);
            });

            Ok(serde_json::Value::Null)
        }
        "stop_recording" => {
            SHOULD_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = keyviz::stop();

            let start = std::time::Instant::now();
            while (!ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
                || !AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst))
                && start.elapsed().as_secs() < 10
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            let video_path = VIDEO_PATH.lock().unwrap().clone().ok_or("No video path")?;
            let audio_path = AUDIO_PATH.lock().unwrap().clone().ok_or("No audio path")?;

            let port = start_media_server(video_path, audio_path.clone())?;

            let mouse_positions = MOUSE_POSITIONS.lock().drain(..).collect::<Vec<_>>();

            let video_url = format!("http://localhost:{}/video", port);
            let audio_url = format!("http://localhost:{}/audio", port);

            let audio_file_path = audio_path;

            Ok(serde_json::json!([video_url, audio_url, mouse_positions, audio_file_path]))
        }
        "get_hotkeys" => {
            let app = APP.lock().unwrap();
            Ok(serde_json::to_value(&app.config.screen_record_hotkeys).unwrap())
        }
        "remove_hotkey" => {
            let index = args["index"].as_u64().ok_or("Missing index")? as usize;
            {
                let mut app = APP.lock().unwrap();
                if index < app.config.screen_record_hotkeys.len() {
                    app.config.screen_record_hotkeys.remove(index);
                    crate::config::save_config(&app.config);
                }
            }
            trigger_hotkey_reload();
            Ok(serde_json::Value::Null)
        }
        "set_hotkey" => {
            let code_str = args["code"].as_str().ok_or("Missing code")?;
            let mods_arr = args["modifiers"].as_array().ok_or("Missing modifiers")?;
            let key_name = args["key"].as_str().unwrap_or("Unknown");

            let vk_code =
                js_code_to_vk(code_str).ok_or(format!("Unsupported key code: {}", code_str))?;

            let mut modifiers = 0;
            for m in mods_arr {
                match m.as_str() {
                    Some("Control") => modifiers |= MOD_CONTROL,
                    Some("Alt") => modifiers |= MOD_ALT,
                    Some("Shift") => modifiers |= MOD_SHIFT,
                    Some("Meta") => modifiers |= MOD_WIN,
                    _ => {}
                }
            }

            {
                let app = APP.lock().unwrap();
                if let Some(msg) = app.config.check_hotkey_conflict(vk_code, modifiers, None) {
                    return Err(msg);
                }
            }

            let mut name_parts = Vec::new();
            if (modifiers & MOD_CONTROL) != 0 {
                name_parts.push("Ctrl");
            }
            if (modifiers & MOD_ALT) != 0 {
                name_parts.push("Alt");
            }
            if (modifiers & MOD_SHIFT) != 0 {
                name_parts.push("Shift");
            }
            if (modifiers & MOD_WIN) != 0 {
                name_parts.push("Win");
            }

            let formatted_key = if key_name.len() == 1 {
                key_name.to_uppercase()
            } else {
                match key_name {
                    " " => "Space".to_string(),
                    _ => key_name.to_string(),
                }
            };
            name_parts.push(&formatted_key);

            let hotkey = Hotkey {
                code: vk_code,
                modifiers,
                name: name_parts.join(" + "),
            };

            {
                let mut app = APP.lock().unwrap();
                app.config.screen_record_hotkeys.push(hotkey.clone());
                crate::config::save_config(&app.config);
            }

            trigger_hotkey_reload();

            Ok(serde_json::to_value(&hotkey).unwrap())
        }
        "unregister_hotkeys" => {
            unsafe {
                if let Ok(hwnd) = FindWindowW(
                    windows::core::w!("HotkeyListenerClass"),
                    windows::core::w!("Listener"),
                ) {
                    if !hwnd.is_invalid() {
                        let _ = PostMessageW(Some(hwnd), WM_UNREGISTER_HOTKEYS, WPARAM(0), LPARAM(0));
                    }
                }
            }
            Ok(serde_json::Value::Null)
        }
        "register_hotkeys" => {
            unsafe {
                if let Ok(hwnd) = FindWindowW(
                    windows::core::w!("HotkeyListenerClass"),
                    windows::core::w!("Listener"),
                ) {
                    if !hwnd.is_invalid() {
                        let _ = PostMessageW(Some(hwnd), WM_REGISTER_HOTKEYS, WPARAM(0), LPARAM(0));
                    }
                }
            }
            Ok(serde_json::Value::Null)
        }
        "minimize_window" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    let _ = ShowWindow(hwnd.0, SW_MINIMIZE);
                }
            }
            Ok(serde_json::Value::Null)
        }
        "toggle_maximize" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    if IsZoomed(hwnd.0).as_bool() {
                        let _ = ShowWindow(hwnd.0, SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd.0, SW_MAXIMIZE);
                    }
                }
            }
            Ok(serde_json::Value::Null)
        }
        "close_window" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    let _ = ShowWindow(hwnd.0, SW_HIDE);
                }
            }
            Ok(serde_json::Value::Null)
        }
        "get_font_css" => {
            let css = crate::overlay::html_components::font_manager::get_font_css();
            Ok(serde_json::json!(css))
        }
        "is_maximized" => unsafe {
            let hwnd = std::ptr::addr_of!(SR_HWND).read();
            let maximized = if !hwnd.is_invalid() {
                IsZoomed(hwnd.0).as_bool()
            } else {
                false
            };
            Ok(serde_json::json!(maximized))
        },
        _ => Err(format!("Unknown command: {}", cmd)),
    }
}

fn fmt_num(v: f32) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").exists()
        && path.join("screen-record").exists()
        && path.join("src").exists()
}

fn find_repo_root() -> Result<PathBuf, String> {
    let mut dir = std::env::current_dir().map_err(|e| format!("current_dir failed: {}", e))?;
    for _ in 0..6 {
        if is_repo_root(&dir) {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    Err("Could not locate repository root".to_string())
}

fn sanitize_svg_rel_path(src: &str) -> Result<String, String> {
    if !src.ends_with(".svg") {
        return Err("Only .svg files are allowed".to_string());
    }
    let rel = src.trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') {
        return Err("Invalid svg path".to_string());
    }
    if !(rel.starts_with("cursor-") || rel.starts_with("cursors/")) {
        return Err("Path outside cursor assets".to_string());
    }
    Ok(rel.to_string())
}

fn apply_cursor_svg_adjustment(
    src: &str,
    scale: f32,
    offset_x_lab: f32,
    offset_y_lab: f32,
) -> Result<usize, String> {
    let rel = sanitize_svg_rel_path(src)?;
    let repo_root = find_repo_root()?;

    let targets = [
        repo_root.join("screen-record").join("public").join(&rel),
        repo_root
            .join("src")
            .join("overlay")
            .join("screen_record")
            .join("dist")
            .join(&rel),
    ];

    let scale = scale.clamp(0.2, 4.0);
    let offset_x = offset_x_lab * (44.0 / 86.0);
    let offset_y = offset_y_lab * (43.0 / 86.0);
    let draw_w = 44.0 * scale;
    let draw_h = 43.0 * scale;
    let x = offset_x + (44.0 - draw_w) * 0.5;
    let y = offset_y + (43.0 - draw_h) * 0.5;

    let re_outer = Regex::new(
        r#"<svg x="[-0-9.]+" y="[-0-9.]+" width="[-0-9.]+" height="[-0-9.]+" viewBox=""#,
    )
    .map_err(|e| format!("regex error: {}", e))?;
    let re_inner =
        Regex::new(r#"transform="translate\([^)]+\)" data-sgt-offset="1""#).map_err(|e| e.to_string())?;

    let replacement = format!(
        r#"<svg x="{}" y="{}" width="{}" height="{}" viewBox=""#,
        fmt_num(x),
        fmt_num(y),
        fmt_num(draw_w),
        fmt_num(draw_h)
    );

    let mut updated = 0usize;
    for path in targets {
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path).map_err(|e| format!("read {:?} failed: {}", path, e))?;
        let next = re_outer.replace(&content, replacement.as_str()).to_string();
        let next = re_inner
            .replace_all(&next, r#"transform="translate(0 0)" data-sgt-offset="1""#)
            .to_string();
        fs::write(&path, next).map_err(|e| format!("write {:?} failed: {}", path, e))?;
        updated += 1;
    }

    if updated == 0 {
        return Err(format!("No target files found for {}", rel));
    }
    Ok(updated)
}

fn trigger_hotkey_reload() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(
            windows::core::w!("HotkeyListenerClass"),
            windows::core::w!("Listener"),
        ) {
            if !hwnd.is_invalid() {
                let _ = PostMessageW(Some(hwnd), WM_RELOAD_HOTKEYS, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn js_code_to_vk(code: &str) -> Option<u32> {
    match code {
        c if c.starts_with("Key") => {
            let chars: Vec<char> = c.chars().collect();
            if chars.len() == 4 {
                Some(chars[3] as u32)
            } else {
                None
            }
        }
        c if c.starts_with("Digit") => {
            let chars: Vec<char> = c.chars().collect();
            if chars.len() == 6 {
                Some(chars[5] as u32)
            } else {
                None
            }
        }
        c if c.starts_with("F") && c.len() <= 3 => c[1..].parse::<u32>().ok().map(|n| 0x70 + n - 1),
        "Space" => Some(0x20),
        "Enter" => Some(0x0D),
        "Escape" => Some(0x1B),
        "Backspace" => Some(0x08),
        "Tab" => Some(0x09),
        "Delete" => Some(0x2E),
        "Insert" => Some(0x2D),
        "Home" => Some(0x24),
        "End" => Some(0x23),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        "ArrowUp" => Some(0x26),
        "ArrowDown" => Some(0x28),
        "ArrowLeft" => Some(0x25),
        "ArrowRight" => Some(0x27),
        "Backquote" => Some(0xC0),
        "Minus" => Some(0xBD),
        "Equal" => Some(0xBB),
        "BracketLeft" => Some(0xDB),
        "BracketRight" => Some(0xDD),
        "Backslash" => Some(0xDC),
        "Semicolon" => Some(0xBA),
        "Quote" => Some(0xDE),
        "Comma" => Some(0xBC),
        "Period" => Some(0xBE),
        "Slash" => Some(0xBF),
        c if c.starts_with("Numpad") => {
            let chars: Vec<char> = c.chars().collect();
            if chars.len() == 7 {
                Some(chars[6] as u32 + 0x30)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn start_media_server(video_path: String, audio_path: String) -> Result<u16, String> {
    let mut port = 8000;
    let server = loop {
        match Server::http(format!("127.0.0.1:{}", port)) {
            Ok(s) => break s,
            Err(_) => {
                port += 1;
                if port > 9000 {
                    return Err("No port available".to_string());
                }
            }
        }
    };

    let actual_port = port;
    SERVER_PORT.store(actual_port, std::sync::atomic::Ordering::SeqCst);

    thread::spawn(move || {
        for request in server.incoming_requests() {
            if request.method() == &tiny_http::Method::Options {
                let mut res = Response::empty(204);
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap(),
                );
                res.add_header(
                    tiny_http::Header::from_bytes(
                        &b"Access-Control-Allow-Methods"[..],
                        &b"GET, OPTIONS"[..],
                    )
                    .unwrap(),
                );
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Range"[..])
                        .unwrap(),
                );
                let _ = request.respond(res);
                continue;
            }

            let url = request.url();
            let is_audio = url.contains("audio");
            let media_path = if is_audio { &audio_path } else { &video_path };
            let content_type = if is_audio { "audio/wav" } else { "video/mp4" };

            if let Ok(file) = File::open(media_path) {
                let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
                let mut start = 0;
                let mut end = file_size.saturating_sub(1);

                if let Some(range) = request
                    .headers()
                    .iter()
                    .find(|h| h.field.as_str() == "Range")
                {
                    if let Some(r) = range.value.as_str().strip_prefix("bytes=") {
                        let parts: Vec<&str> = r.split('-').collect();
                        if parts.len() == 2 {
                            if let Ok(s) = parts[0].parse::<u64>() {
                                start = s;
                            }
                            if let Ok(e) = parts[1].parse::<u64>() {
                                if !parts[1].is_empty() {
                                    end = e;
                                }
                            }
                        }
                    }
                }

                if let Ok(mut f) = File::open(media_path) {
                    let _ = f.seek(std::io::SeekFrom::Start(start));
                    let mut res = Response::new(
                        if start == 0 && end == file_size.saturating_sub(1) {
                            StatusCode(200)
                        } else {
                            StatusCode(206)
                        },
                        vec![
                            tiny_http::Header::from_bytes(
                                &b"Content-Type"[..],
                                content_type.as_bytes(),
                            )
                            .unwrap(),
                            tiny_http::Header::from_bytes(
                                &b"Access-Control-Allow-Origin"[..],
                                &b"*"[..],
                            )
                            .unwrap(),
                        ],
                        Box::new(f.take(end - start + 1)) as Box<dyn Read + Send>,
                        Some((end - start + 1) as usize),
                        None,
                    );
                    if start != 0 || end != file_size.saturating_sub(1) {
                        res.add_header(
                            tiny_http::Header::from_bytes(
                                &b"Content-Range"[..],
                                format!("bytes {}-{}/{}", start, end, file_size).as_bytes(),
                            )
                            .unwrap(),
                        );
                    }
                    let _ = request.respond(res);
                }
            } else {
                let _ = request.respond(Response::from_string("File not found").with_status_code(404));
            }
        }
    });

    Ok(actual_port)
}
