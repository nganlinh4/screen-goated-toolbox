// --- SCREEN RECORD IPC ---
// IPC command handling for screen recording WebView.

use super::bg_download;
use super::engine::{
    get_monitors, CaptureHandler, ACTIVE_CAPTURE_CONTROL, AUDIO_ENCODING_FINISHED, CAPTURE_ERROR,
    ENCODER_ACTIVE, ENCODING_FINISHED, MOUSE_POSITIONS, SHOULD_STOP, VIDEO_PATH,
};
use super::input_capture;
use super::mf_decode;
use super::native_export;
use super::raw_video;
use super::{SERVER_PORT, SR_HWND};
use crate::config::Hotkey;
use crate::APP;
use base64::Engine as _;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::thread;
use tiny_http::{Response, Server, StatusCode};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::UI::Shell::ExtractIconExW;
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

fn get_process_exe_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);

        if result.is_ok() && size > 0 {
            Some(String::from_utf16_lossy(&buffer[..size as usize]))
        } else {
            None
        }
    }
}

fn extract_icon_data_url_from_exe(exe_path: &str) -> Option<String> {
    unsafe {
        let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut large_icon = HICON::default();
        let count = ExtractIconExW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            0,
            Some(&mut large_icon),
            None,
            1,
        );
        if count == 0 || large_icon.is_invalid() {
            return None;
        }

        let mut icon_info = ICONINFO::default();
        if GetIconInfo(large_icon, &mut icon_info).is_err() {
            let _ = DestroyIcon(large_icon);
            return None;
        }

        let mut bmp = BITMAP::default();
        if GetObjectW(
            icon_info.hbmColor.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&mut bmp as *mut BITMAP).cast()),
        ) == 0
        {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            let _ = DestroyIcon(large_icon);
            return None;
        }

        let width = bmp.bmWidth as u32;
        let height = bmp.bmHeight as u32;
        if width == 0 || height == 0 {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            let _ = DestroyIcon(large_icon);
            return None;
        }

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let lines = GetDIBits(
            hdc_mem,
            icon_info.hbmColor,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut std::ffi::c_void),
            &bmi as *const _ as *mut _,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
        let _ = DeleteObject(icon_info.hbmMask.into());
        let _ = DeleteObject(icon_info.hbmColor.into());
        let _ = DestroyIcon(large_icon);

        if lines == 0 {
            return None;
        }

        let mut has_alpha = false;
        for i in (0..pixels.len()).step_by(4) {
            pixels.swap(i, i + 2);
            if pixels[i + 3] != 0 {
                has_alpha = true;
            }
        }
        if !has_alpha {
            for i in (3..pixels.len()).step_by(4) {
                pixels[i] = 255;
            }
        }

        let rgba_image = image::RgbaImage::from_raw(width, height, pixels)?;
        let mut png_data = Vec::<u8>::new();
        rgba_image
            .write_to(
                &mut std::io::Cursor::new(&mut png_data),
                image::ImageFormat::Png,
            )
            .ok()?;
        let base64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
        Some(format!("data:image/png;base64,{}", base64))
    }
}

fn capture_window_thumbnail(hwnd: HWND) -> Option<String> {
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
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return None;
            }
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return None;
        }

        let max_dim = 250.0f32;
        let scale = if width > height {
            max_dim / width as f32
        } else {
            max_dim / height as f32
        }
        .min(1.0);
        let t_width = ((width as f32 * scale).round() as i32).max(1);
        let t_height = ((height as f32 * scale).round() as i32).max(1);

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);
        if hbitmap.0.is_null() {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return None;
        }

        let old_obj = SelectObject(hdc_mem, hbitmap.into());
        let pw_renderfullcontent = 2u32;
        let print_ok =
            PrintWindow(hwnd, hdc_mem, PRINT_WINDOW_FLAGS(pw_renderfullcontent)).as_bool();
        if !print_ok {
            let _ = SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(hbitmap.into());
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return None;
        }

        let hdc_thumb = CreateCompatibleDC(Some(hdc_screen));
        let hbitmap_thumb = CreateCompatibleBitmap(hdc_screen, t_width, t_height);
        if hbitmap_thumb.0.is_null() {
            let _ = SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(hbitmap.into());
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            let _ = DeleteDC(hdc_thumb);
            return None;
        }
        let old_thumb = SelectObject(hdc_thumb, hbitmap_thumb.into());

        let _ = SetStretchBltMode(hdc_thumb, HALFTONE);
        let _ = StretchBlt(
            hdc_thumb,
            0,
            0,
            t_width,
            t_height,
            Some(hdc_mem),
            0,
            0,
            width,
            height,
            SRCCOPY,
        );

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: t_width,
                biHeight: -t_height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (t_width * t_height * 4) as usize];
        let lines = GetDIBits(
            hdc_thumb,
            hbitmap_thumb,
            0,
            t_height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(hdc_thumb, old_thumb);
        let _ = DeleteObject(hbitmap_thumb.into());
        let _ = DeleteDC(hdc_thumb);

        let _ = SelectObject(hdc_mem, old_obj);
        let _ = DeleteObject(hbitmap.into());
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);

        if lines == 0 {
            return None;
        }

        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
            chunk[3] = 255;
        }

        let rgba_image = image::RgbaImage::from_raw(t_width as u32, t_height as u32, pixels)?;
        let mut jpeg_data = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_data, 75);
        if enc
            .encode_image(&image::DynamicImage::ImageRgba8(rgba_image))
            .is_ok()
        {
            Some(format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&jpeg_data)
            ))
        } else {
            None
        }
    }
}

/// Capture a live screenshot of a monitor region and return as a JPEG data URL.
/// Uses `BitBlt` from the desktop DC — same pattern as `capture_window_thumbnail`.
fn capture_monitor_thumbnail(x: i32, y: i32, width: i32, height: i32) -> Option<String> {
    if width <= 0 || height <= 0 {
        return None;
    }
    unsafe {
        let max_dim = 300.0f32;
        let scale = (max_dim / width.max(height) as f32).min(1.0);
        let t_w = ((width as f32 * scale).round() as i32).max(1);
        let t_h = ((height as f32 * scale).round() as i32).max(1);

        let hdc_screen = GetDC(None);
        let hdc_thumb = CreateCompatibleDC(Some(hdc_screen));
        let hbitmap = CreateCompatibleBitmap(hdc_screen, t_w, t_h);
        if hbitmap.0.is_null() {
            let _ = DeleteDC(hdc_thumb);
            let _ = ReleaseDC(None, hdc_screen);
            return None;
        }
        let old = SelectObject(hdc_thumb, hbitmap.into());
        let _ = SetStretchBltMode(hdc_thumb, HALFTONE);
        let _ = StretchBlt(hdc_thumb, 0, 0, t_w, t_h, Some(hdc_screen), x, y, width, height, SRCCOPY);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: t_w,
                biHeight: -t_h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (t_w * t_h * 4) as usize];
        let lines = GetDIBits(
            hdc_thumb,
            hbitmap,
            0,
            t_h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = SelectObject(hdc_thumb, old);
        let _ = DeleteObject(hbitmap.into());
        let _ = DeleteDC(hdc_thumb);
        let _ = ReleaseDC(None, hdc_screen);

        if lines == 0 {
            return None;
        }
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2); // BGRA → RGBA
            chunk[3] = 255;
        }
        let rgba = image::RgbaImage::from_raw(t_w as u32, t_h as u32, pixels)?;
        let mut jpeg_data = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_data, 80);
        if enc
            .encode_image(&image::DynamicImage::ImageRgba8(rgba))
            .is_ok()
        {
            Some(format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&jpeg_data)
            ))
        } else {
            None
        }
    }
}

fn gather_window_infos() -> Result<Vec<serde_json::Value>, String> {
    let windows =
        windows_capture::window::Window::enumerate().map_err(|e| e.to_string())?;
    let mut window_infos = Vec::new();
    for window in windows {
        if !window.is_valid() {
            continue;
        }
        let Ok(title) = window.title() else {
            continue;
        };
        if title.trim().is_empty() {
            continue;
        }
        let process_name = window.process_name().unwrap_or_default();
        let hwnd_val = window.as_raw_hwnd() as usize;
        let preview_data_url =
            capture_window_thumbnail(HWND(hwnd_val as *mut std::ffi::c_void));
        let icon_data_url = window
            .process_id()
            .ok()
            .and_then(get_process_exe_path)
            .and_then(|path| extract_icon_data_url_from_exe(&path));
        let mut is_admin = false;
        if let Ok(pid) = window.process_id() {
            let handle =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
            if handle.is_err() {
                is_admin = true;
            } else if let Ok(h) = handle {
                unsafe {
                    let _ = CloseHandle(h);
                }
            }
        }
        let is_admin_gated = is_admin && preview_data_url.is_none();
        window_infos.push(serde_json::json!({
            "id": hwnd_val.to_string(),
            "title": title,
            "processName": process_name,
            "isAdmin": is_admin_gated,
            "iconDataUrl": icon_data_url,
            "previewDataUrl": preview_data_url,
        }));
    }
    Ok(window_infos)
}

/// Fast metadata-only enumeration — no thumbnail capture.
/// Returns each window with `winW`/`winH` for aspect ratio display.
fn gather_window_metadata() -> Result<Vec<serde_json::Value>, String> {
    let windows =
        windows_capture::window::Window::enumerate().map_err(|e| e.to_string())?;
    let mut infos = Vec::new();
    for window in windows {
        if !window.is_valid() {
            continue;
        }
        let Ok(title) = window.title() else {
            continue;
        };
        if title.trim().is_empty() {
            continue;
        }
        let process_name = window.process_name().unwrap_or_default();
        let hwnd_val = window.as_raw_hwnd() as usize;
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

        // Get actual window dimensions for correct aspect ratio.
        let (win_w, win_h) = unsafe {
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
            (
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
            )
        };

        let icon_data_url = window
            .process_id()
            .ok()
            .and_then(get_process_exe_path)
            .and_then(|path| extract_icon_data_url_from_exe(&path));
        let mut is_admin = false;
        if let Ok(pid) = window.process_id() {
            let handle =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
            if handle.is_err() {
                is_admin = true;
            } else if let Ok(h) = handle {
                unsafe { let _ = CloseHandle(h); }
            }
        }
        infos.push(serde_json::json!({
            "id": hwnd_val.to_string(),
            "title": title,
            "processName": process_name,
            "isAdmin": is_admin,
            "iconDataUrl": icon_data_url,
            "previewDataUrl": serde_json::Value::Null,
            "winW": win_w,
            "winH": win_h,
        }));
    }
    Ok(infos)
}

pub fn handle_ipc_command(
    cmd: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match cmd.as_str() {
        "check_bg_downloaded" => {
            let id = args["id"].as_str().unwrap_or("");
            let info = bg_download::download_info(id);
            Ok(serde_json::json!({
                "downloaded": info.is_some(),
                "ext": info.as_ref().map(|(ext, _)| ext.clone()),
                "version": info.as_ref().map(|(_, version)| *version)
            }))
        }
        "start_bg_download" => {
            let id = args["id"].as_str().unwrap_or("").to_string();
            let url = args["url"].as_str().unwrap_or("").to_string();
            bg_download::start_download(id, url);
            Ok(serde_json::Value::Null)
        }
        "get_bg_download_progress" => {
            let id = args["id"].as_str().unwrap_or("");
            let status = bg_download::get_download_status(id);
            Ok(serde_json::to_value(&status).unwrap())
        }
        "delete_bg_download" => {
            let id = args["id"].as_str().unwrap_or("");
            bg_download::delete_downloaded(id);
            Ok(serde_json::Value::Null)
        }
        "read_bg_as_data_url" => {
            let id = args["id"].as_str().unwrap_or("");
            match bg_download::read_as_data_url(id) {
                Ok(data_url) => Ok(serde_json::json!(data_url)),
                Err(e) => Err(e),
            }
        }
        "save_uploaded_bg_data_url" => {
            let data_url = args["dataUrl"].as_str().ok_or("Missing dataUrl")?;
            let url = bg_download::save_uploaded_data_url(data_url)?;
            Ok(serde_json::json!(url))
        }
        "prewarm_custom_background" => {
            let url = args["url"].as_str().ok_or("Missing url")?;
            native_export::prewarm_custom_background(url)?;
            Ok(serde_json::Value::Null)
        }
        "log_message" => {
            let msg = args["message"].as_str().unwrap_or("");
            eprintln!("{msg}");
            Ok(serde_json::Value::Null)
        }
        "clear_export_staging" => {
            native_export::staging::clear_staged();
            Ok(serde_json::Value::Null)
        }
        "stage_export_data" => {
            let data_type = args["dataType"].as_str().ok_or("missing dataType")?;
            match data_type {
                "camera" => {
                    let frames: Vec<native_export::config::BakedCameraFrame> =
                        serde_json::from_value(args["data"].clone())
                            .map_err(|e| format!("bad camera chunk: {e}"))?;
                    native_export::staging::append_camera_frames(frames);
                }
                "cursor" => {
                    let frames: Vec<native_export::config::BakedCursorFrame> =
                        serde_json::from_value(args["data"].clone())
                            .map_err(|e| format!("bad cursor chunk: {e}"))?;
                    native_export::staging::append_cursor_frames(frames);
                }
                "atlas" => {
                    let b64 = args["base64"].as_str().ok_or("missing base64")?;
                    let w = args["width"].as_u64().unwrap_or(1) as u32;
                    let h = args["height"].as_u64().unwrap_or(1) as u32;
                    let raw = base64::engine::general_purpose::STANDARD
                        .decode(b64.trim_start_matches("data:image/png;base64,"))
                        .map_err(|e| e.to_string())?;
                    let img = image::load_from_memory(&raw)
                        .map_err(|e| e.to_string())?
                        .to_rgba8();
                    native_export::staging::set_atlas(img.into_raw(), w, h);
                }
                "overlay_frames_chunk" => {
                    let frames: Vec<native_export::config::OverlayFrame> =
                        serde_json::from_value(args["data"].clone())
                            .map_err(|e| format!("bad overlay chunk: {e}"))?;
                    native_export::staging::append_overlay_frames(frames);
                }
                _ => return Err(format!("unknown stage dataType: {data_type}")),
            }
            Ok(serde_json::Value::Null)
        }
        "start_export_server" => {
            let result = native_export::start_native_export(args);
            native_export::persist_export_result(&result);
            result
        }
        "get_export_capabilities" => Ok(native_export::get_export_capabilities()),
        "cancel_export" => {
            println!("[Cancel] IPC cancel_export received");
            native_export::cancel_export();
            println!("[Cancel] cancel_export() returned");
            Ok(serde_json::Value::Null)
        }
        "get_default_export_dir" => Ok(serde_json::json!(native_export::get_default_export_dir())),
        "get_media_server_port" => {
            let mut port = SERVER_PORT.load(std::sync::atomic::Ordering::SeqCst);
            if port == 0 {
                port = start_global_media_server().unwrap_or(0);
            }
            Ok(serde_json::json!(port))
        }
        "generate_thumbnails" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let count = args["count"].as_u64().unwrap_or(20) as u32;
            let start = args["start"].as_f64().unwrap_or(0.0);
            let end = args["end"].as_f64().unwrap_or(start);
            let result = super::mf_decode::generate_thumbnails(path, count, start, end)?;
            Ok(serde_json::json!(result))
        }
        "probe_video_metadata" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let result = mf_decode::probe_video_metadata(path)?;
            Ok(serde_json::to_value(result).unwrap())
        }
        "show_in_folder" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            std::process::Command::new("explorer")
                .args(["/select,", &path.replace("/", "\\")])
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(serde_json::Value::Null)
        }
        "rename_file" => {
            let path_str = args["path"].as_str().ok_or("Missing path")?;
            let new_name = args["newName"].as_str().ok_or("Missing newName")?;
            let path = std::path::PathBuf::from(path_str);
            let new_path = path.with_file_name(new_name);
            std::fs::rename(&path, &new_path).map_err(|e| e.to_string())?;
            Ok(serde_json::json!(new_path.to_string_lossy().to_string()))
        }
        "delete_file" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let _ = std::fs::remove_file(path);
            Ok(serde_json::Value::Null)
        }
        "pick_export_folder" => {
            let initial_dir = args["initialDir"].as_str().map(|s| s.to_string());
            let selected = native_export::pick_export_folder(initial_dir)?;
            Ok(match selected {
                Some(path) => serde_json::json!(path),
                None => serde_json::Value::Null,
            })
        }
        "save_raw_video_copy" => {
            let source_path = args["sourcePath"].as_str().ok_or("Missing sourcePath")?;
            let target_dir = args["targetDir"].as_str().ok_or("Missing targetDir")?;
            let saved_path = raw_video::save_raw_video_copy(source_path, target_dir)?;
            Ok(serde_json::json!({ "savedPath": saved_path }))
        }
        "move_saved_raw_video" => {
            let current_path = args["currentPath"].as_str().ok_or("Missing currentPath")?;
            let target_dir = args["targetDir"].as_str().ok_or("Missing targetDir")?;
            let saved_path = raw_video::move_saved_raw_video(current_path, target_dir)?;
            Ok(serde_json::json!({ "savedPath": saved_path }))
        }
        "copy_video_file_to_clipboard" => {
            let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
            raw_video::copy_video_file_to_clipboard(file_path)?;
            Ok(serde_json::Value::Null)
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
            let mut monitors = get_monitors();
            for m in &mut monitors {
                m.thumbnail = capture_monitor_thumbnail(m.x, m.y, m.width as i32, m.height as i32);
            }
            Ok(serde_json::to_value(monitors).unwrap())
        }
        "get_windows" => Ok(serde_json::Value::Array(gather_window_infos()?)),
        "show_window_selector" => {
            // Fast: gather metadata only (no thumbnails) so the overlay opens instantly.
            let window_infos = gather_window_metadata()?;
            let (is_dark, lang) = {
                let app = crate::APP.lock().unwrap();
                let is_dark = match app.config.theme_mode {
                    crate::config::ThemeMode::Dark => true,
                    crate::config::ThemeMode::Light => false,
                    crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
                };
                let lang = app.config.ui_language.clone();
                (is_dark, lang)
            };
            super::window_selection::show_window_selector(window_infos.clone(), is_dark, lang);

            // Lazy-load thumbnails in background — push them one-by-one after UI appears.
            std::thread::spawn(move || {
                // Wait for the WebView to finish its entrance animation before flooding it.
                std::thread::sleep(std::time::Duration::from_millis(280));
                for win in &window_infos {
                    if super::window_selection::selector_is_closed() {
                        break;
                    }
                    let hwnd_val = win["id"]
                        .as_str()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if hwnd_val == 0 {
                        continue;
                    }
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    if let Some(data_url) = capture_window_thumbnail(hwnd) {
                        super::window_selection::post_thumbnail_update(hwnd_val, data_url);
                    }
                }
            });

            Ok(serde_json::Value::Null)
        }
        "start_recording" => {
            let target_type = args["targetType"].as_str().unwrap_or("monitor");
            let target_id = args["targetId"]
                .as_str()
                .or_else(|| args["monitorId"].as_str())
                .unwrap_or("0");
            let include_cursor = args["includeCursor"].as_bool().unwrap_or(false);
            let cursor_setting = if include_cursor {
                CursorCaptureSettings::WithCursor
            } else {
                CursorCaptureSettings::WithoutCursor
            };

            SHOULD_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
            super::engine::CURSOR_SIGNATURE_CACHE.lock().clear();
            MOUSE_POSITIONS.lock().clear();
            ACTIVE_CAPTURE_CONTROL.lock().take();
            super::engine::EXTERNAL_CAPTURE_CONTROL.lock().take();
            *CAPTURE_ERROR.lock() = None;

            let fps: Option<u32> = args["fps"].as_u64().map(|v| v as u32);
            let flag_str = serde_json::to_string(&serde_json::json!({
                "target_type": target_type,
                "target_id": target_id,
                "fps": fps,
            }))
            .unwrap();

            eprintln!(
                "[CaptureBackend] start_recording: target_type={:?}, target_id={:?}",
                target_type, target_id
            );

            // Request 1ms timer resolution so thread::sleep(1ms) actually sleeps ~1ms
            // instead of the default ~15.6ms Windows scheduler quantum.
            unsafe { timeBeginPeriod(1); }

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
                    unsafe {
                        windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool()
                    }
                );

                if hwnd_val == 0
                    || !unsafe {
                        windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool()
                    }
                {
                    return Err(format!("Invalid window handle: 0x{:X}", hwnd_val));
                }

                let window = windows_capture::window::Window::from_raw_hwnd(
                    hwnd_val as *mut std::ffi::c_void,
                );

                super::engine::TARGET_HWND
                    .store(hwnd_val, std::sync::atomic::Ordering::Relaxed);

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
                    super::engine::MONITOR_X = rect.left;
                    super::engine::MONITOR_Y = rect.top;
                }

                let update_interval = if let Some(f) = fps {
                    let target_micros = 1_000_000 / f.max(1);
                    MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_micros((target_micros / 2) as u64))
                } else {
                    MinimumUpdateIntervalSettings::Default
                };

                let settings = Settings::new(
                    window,
                    cursor_setting,
                    DrawBorderSettings::WithoutBorder,
                    SecondaryWindowSettings::Default,
                    update_interval,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flag_str,
                );

                match CaptureHandler::start_free_threaded(settings) {
                    Ok(control) => {
                        *super::engine::EXTERNAL_CAPTURE_CONTROL.lock() = Some(control);
                    }
                    Err(e) => {
                        let msg = format!("Window capture failed: {}", e);
                        eprintln!("[CaptureBackend] {}", msg);
                        *CAPTURE_ERROR.lock() = Some(msg.clone());
                        return Err(msg);
                    }
                }

                // Show a distinct blue border around the captured window.
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
                    super::capture_border::show_capture_border(
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                    );
                }
            } else {
                super::engine::TARGET_HWND.store(0, std::sync::atomic::Ordering::Relaxed);
                let monitor_index = target_id.parse::<usize>().unwrap_or(0);
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
                        let mut info: windows::Win32::Graphics::Gdi::MONITORINFOEXW =
                            std::mem::zeroed();
                        info.monitorInfo.cbSize = std::mem::size_of::<
                            windows::Win32::Graphics::Gdi::MONITORINFOEXW,
                        >() as u32;
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

                let update_interval = if let Some(f) = fps {
                    let target_micros = 1_000_000 / f.max(1);
                    MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_micros((target_micros / 2) as u64))
                } else {
                    MinimumUpdateIntervalSettings::Default
                };

                let settings = Settings::new(
                    monitor,
                    cursor_setting,
                    DrawBorderSettings::Default,
                    SecondaryWindowSettings::Include,
                    update_interval,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flag_str,
                );

                match CaptureHandler::start_free_threaded(settings) {
                    Ok(control) => {
                        *super::engine::EXTERNAL_CAPTURE_CONTROL.lock() = Some(control);
                    }
                    Err(e) => {
                        let msg = format!("Display capture failed: {}", e);
                        eprintln!("[CaptureBackend] {}", msg);
                        *CAPTURE_ERROR.lock() = Some(msg.clone());
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
        "stop_recording" => {
            SHOULD_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
            super::engine::TARGET_HWND.store(0, std::sync::atomic::Ordering::SeqCst);
            super::capture_border::hide_capture_border();

            // Restore default timer resolution (matching the timeBeginPeriod in start_recording).
            unsafe { timeEndPeriod(1); }
            if let Some(control) = ACTIVE_CAPTURE_CONTROL.lock().take() {
                control.stop();
            }
            let raw_input_events = input_capture::stop_capture_and_drain();

            // Check if capture failed to start (error stored by the capture thread).
            // Give the capture thread a brief moment to report failure.
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Some(err_msg) = CAPTURE_ERROR.lock().take() {
                // Clean up all recording state so nothing keeps running.
                ENCODER_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
                super::engine::SHOULD_STOP_AUDIO.store(true, std::sync::atomic::Ordering::SeqCst);
                ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
                AUDIO_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
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
                || !AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst))
                && start.elapsed().as_secs() < 10
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            let encoding_done = ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
                && AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst);

            if !encoding_done {
                eprintln!(
                    "[CaptureBackend] Encoding did not finish within timeout. \
                     Stopping capture thread and cleaning up."
                );

                // Force-stop the capture thread so on_closed → shutdown_and_finalize
                // runs.  This is the fallback if on_frame_arrived never fired.
                if let Some(control) = super::engine::EXTERNAL_CAPTURE_CONTROL.lock().take() {
                    let _ = control.stop();
                }

                // Give shutdown_and_finalize's spawned thread a moment to set
                // ENCODING_FINISHED after the capture thread is stopped.
                let retry_start = std::time::Instant::now();
                while (!ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
                    || !AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst))
                    && retry_start.elapsed().as_secs() < 5
                {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }

                let retry_done = ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst)
                    && AUDIO_ENCODING_FINISHED.load(std::sync::atomic::Ordering::SeqCst);

                if !retry_done {
                    ENCODER_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
                    super::engine::SHOULD_STOP_AUDIO
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);
                    AUDIO_ENCODING_FINISHED.store(true, std::sync::atomic::Ordering::SeqCst);

                    if let Some(ref path) = *VIDEO_PATH.lock().unwrap() {
                        let _ = std::fs::remove_file(path);
                    }

                    return Err("Recording failed: encoding did not complete in time. \
                         Please try again."
                        .to_string());
                }
            }

            // Clean up the capture thread now that encoding is done.
            if let Some(control) = super::engine::EXTERNAL_CAPTURE_CONTROL.lock().take() {
                let _ = control.stop();
            }

            let video_path = VIDEO_PATH.lock().unwrap().clone().ok_or("No video path")?;
            let video_file_path = video_path.clone();
            let last_recording_fps = *super::engine::LAST_RECORDING_FPS.lock().unwrap();

            let mut port = SERVER_PORT.load(std::sync::atomic::Ordering::SeqCst);
            if port == 0 {
                port = start_global_media_server().unwrap_or(0);
            }

            let mouse_positions = MOUSE_POSITIONS.lock().drain(..).collect::<Vec<_>>();

            let encoded_path = urlencoding::encode(&video_path);
            let video_url = format!("http://localhost:{}/?path={}", port, encoded_path);
            let audio_url = format!("http://localhost:{}/?path={}", port, encoded_path);

            Ok(serde_json::json!([
                video_url,
                audio_url,
                mouse_positions,
                video_file_path,
                video_file_path,
                raw_input_events,
                last_recording_fps
            ]))
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
                        let _ =
                            PostMessageW(Some(hwnd), WM_UNREGISTER_HOTKEYS, WPARAM(0), LPARAM(0));
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
        "restore_window" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    if IsIconic(hwnd.0).as_bool() {
                        let _ = ShowWindow(hwnd.0, SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd.0, SW_SHOW);
                    }
                    let _ = SetForegroundWindow(hwnd.0);
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
    let offset_x = offset_x_lab;
    let offset_y = offset_y_lab;
    let draw_w = 44.0 * scale;
    let draw_h = 43.0 * scale;
    let x = offset_x + (44.0 - draw_w) * 0.5;
    let y = offset_y + (43.0 - draw_h) * 0.5;

    let mut found = 0usize;
    let mut updated = 0usize;
    for path in targets {
        if !path.exists() {
            continue;
        }
        found += 1;
        let content =
            fs::read_to_string(&path).map_err(|e| format!("read {:?} failed: {}", path, e))?;
        let replaced = replace_cursor_svg_geometry(&content, x, y, draw_w, draw_h, scale)?;
        if replaced != content {
            let next = normalize_sgt_offset_transform(replaced);
            if next != content {
                fs::write(&path, next).map_err(|e| format!("write {:?} failed: {}", path, e))?;
                updated += 1;
            }
        }
    }

    if found == 0 {
        return Err(format!("No target files found for {}", rel));
    }
    Ok(updated)
}

fn replace_cursor_svg_geometry(
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scale: f32,
) -> Result<String, String> {
    if let Ok(next) = replace_nested_svg_geometry(content, x, y, width, height) {
        return Ok(next);
    }
    replace_group_transform_geometry(content, x, y, scale)
}

fn replace_nested_svg_geometry(
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<String, String> {
    let mut cursor = 0usize;
    let mut svg_index = 0usize;
    let mut target: Option<(usize, usize)> = None;

    while let Some(rel) = content[cursor..].find("<svg") {
        let start = cursor + rel;
        let end_rel = content[start..]
            .find('>')
            .ok_or("Could not locate end of nested <svg> tag")?;
        let end = start + end_rel;
        let tag = &content[start..=end];

        if svg_index > 0 && tag.contains("viewBox=") {
            target = Some((start, end));
            break;
        }

        svg_index += 1;
        cursor = end + 1;
    }

    let (start, end) = target.ok_or("Could not locate nested <svg ... viewBox=...> block")?;
    let tag = &content[start..=end];
    let tag = set_or_insert_svg_attr(tag, "x", &fmt_num(x));
    let tag = set_or_insert_svg_attr(&tag, "y", &fmt_num(y));
    let tag = set_or_insert_svg_attr(&tag, "width", &fmt_num(width));
    let tag = set_or_insert_svg_attr(&tag, "height", &fmt_num(height));

    Ok(format!(
        "{}{}{}",
        &content[..start],
        tag,
        &content[end + 1..]
    ))
}

fn set_or_insert_svg_attr(tag: &str, name: &str, value: &str) -> String {
    let double_pat = format!(r#"{}=""#, name);
    if let Some(pos) = tag.find(&double_pat) {
        let value_start = pos + double_pat.len();
        if let Some(end_rel) = tag[value_start..].find('"') {
            let value_end = value_start + end_rel;
            return format!("{}{}{}", &tag[..value_start], value, &tag[value_end..]);
        }
    }

    let single_pat = format!(r#"{}='"#, name);
    if let Some(pos) = tag.find(&single_pat) {
        let value_start = pos + single_pat.len();
        if let Some(end_rel) = tag[value_start..].find('\'') {
            let value_end = value_start + end_rel;
            return format!("{}{}{}", &tag[..value_start], value, &tag[value_end..]);
        }
    }

    if let Some(gt) = tag.rfind('>') {
        return format!(r#"{} {}="{}"{}"#, &tag[..gt], name, value, &tag[gt..]);
    }

    tag.to_string()
}

fn replace_group_transform_geometry(
    content: &str,
    x: f32,
    y: f32,
    scale: f32,
) -> Result<String, String> {
    let marker = r#"<g transform="translate("#;
    let start = content
        .find(marker)
        .ok_or("Could not locate group transform for cursor geometry")?;
    let rest = &content[start..];
    let end_rel = rest
        .find(")\">")
        .ok_or("Could not locate end of group transform")?;
    let end = start + end_rel + 3;

    let replacement = format!(
        r#"<g transform="translate({} {}) scale({})">"#,
        fmt_num(x),
        fmt_num(y),
        fmt_num(scale)
    );

    Ok(format!(
        "{}{}{}",
        &content[..start],
        replacement,
        &content[end..]
    ))
}

fn normalize_sgt_offset_transform(mut content: String) -> String {
    let marker = r#"data-sgt-offset="1""#;
    let transform_prefix = r#"transform="translate("#;
    let transform_replacement = r#"transform="translate(0 0)""#;

    let mut search_from = 0usize;
    while let Some(marker_rel) = content[search_from..].find(marker) {
        let marker_idx = search_from + marker_rel;
        let before = &content[..marker_idx];
        if let Some(ts) = before.rfind(transform_prefix) {
            let after_ts = &content[ts..];
            if let Some(end_rel) = after_ts.find(")\"") {
                let end = ts + end_rel + 2; // include )"
                content.replace_range(ts..end, transform_replacement);
                search_from = marker_idx + marker.len();
                continue;
            }
        }
        search_from = marker_idx + marker.len();
    }
    content
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

pub fn start_global_media_server() -> Result<u16, String> {
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
        for mut request in server.incoming_requests() {
            if request.method() == &tiny_http::Method::Options {
                let mut res = Response::empty(204);
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap(),
                );
                res.add_header(
                    tiny_http::Header::from_bytes(
                        &b"Access-Control-Allow-Methods"[..],
                        &b"GET, POST, OPTIONS"[..],
                    )
                    .unwrap(),
                );
                res.add_header(
                    tiny_http::Header::from_bytes(
                        &b"Access-Control-Allow-Headers"[..],
                        &b"Range, Content-Type"[..],
                    )
                    .unwrap(),
                );
                let _ = request.respond(res);
                continue;
            }

            // POST /write-temp — write binary body to recordings dir, return file path.
            // Used to restore rawVideoPath for old projects that only have a blob.
            if request.method() == &tiny_http::Method::Post
                && request.url().starts_with("/write-temp")
            {
                let cors = tiny_http::Header::from_bytes(
                    &b"Access-Control-Allow-Origin"[..],
                    &b"*"[..],
                )
                .unwrap();
                let recordings_dir = dirs::data_local_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("screen-goated-toolbox")
                    .join("recordings");
                let _ = std::fs::create_dir_all(&recordings_dir);
                let dest = recordings_dir.join(format!(
                    "restored_{}.mp4",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                ));
                let mut body = Vec::new();
                if request.as_reader().read_to_end(&mut body).is_ok()
                    && !body.is_empty()
                    && std::fs::write(&dest, &body).is_ok()
                {
                    let json =
                        format!("{{\"path\":{}}}", serde_json::json!(dest.to_string_lossy()));
                    let mut res = Response::from_string(json).with_status_code(200);
                    res.add_header(cors);
                    res.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    );
                    let _ = request.respond(res);
                } else {
                    let mut res =
                        Response::from_string("Write failed").with_status_code(500);
                    res.add_header(cors);
                    let _ = request.respond(res);
                }
                continue;
            }

            let url = request.url();
            let media_path_str = if let Some(idx) = url.find("?path=") {
                let encoded = &url[idx + 6..];
                urlencoding::decode(encoded)
                    .unwrap_or_else(|_| std::borrow::Cow::Borrowed(""))
                    .into_owned()
            } else {
                String::new()
            };
            if media_path_str.is_empty() || !Path::new(&media_path_str).exists() {
                let mut res = Response::from_string("File not found").with_status_code(404);
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap(),
                );
                let _ = request.respond(res);
                continue;
            }

            let media_path = Path::new(&media_path_str);
            let content_type = match media_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "wav" => "audio/wav",
                "mp3" => "audio/mpeg",
                "m4a" => "audio/mp4",
                "aac" => "audio/aac",
                _ => "video/mp4",
            };

            if let Ok(file) = File::open(media_path) {
                let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
                if file_size == 0 {
                    let mut res = Response::empty(200);
                    res.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Access-Control-Allow-Origin"[..],
                            &b"*"[..],
                        )
                        .unwrap(),
                    );
                    let _ = request.respond(res);
                    continue;
                }
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
                            tiny_http::Header::from_bytes(
                                &b"Accept-Ranges"[..],
                                &b"bytes"[..],
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
                let mut res = Response::from_string("File not found").with_status_code(404);
                res.add_header(
                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                        .unwrap(),
                );
                let _ = request.respond(res);
            }
        }
    });

    Ok(actual_port)
}
