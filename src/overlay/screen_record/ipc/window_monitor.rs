// --- WINDOW / MONITOR ENUMERATION ---
// Window and monitor thumbnail capture, process icon extraction, and
// window metadata gathering for the screen-record source picker.

use base64::Engine as _;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) fn get_process_exe_path(pid: u32) -> Option<String> {
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

pub(super) fn extract_icon_data_url_from_exe(exe_path: &str) -> Option<String> {
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

pub(crate) fn capture_window_thumbnail(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(hwnd, &mut rect).is_err()
        {
            return None;
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
pub(super) fn capture_monitor_thumbnail(x: i32, y: i32, width: i32, height: i32) -> Option<String> {
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
        let _ = StretchBlt(
            hdc_thumb,
            0,
            0,
            t_w,
            t_h,
            Some(hdc_screen),
            x,
            y,
            width,
            height,
            SRCCOPY,
        );

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

/// Capture a single window thumbnail with a timeout to prevent deadlock.
///
/// `PrintWindow` sends a synchronous `WM_PRINT` to the target app's UI thread.
/// If the target is hung/frozen, it blocks indefinitely. This wrapper runs the
/// capture on a spawned thread with a 500ms deadline to avoid blocking the caller.
pub(super) fn capture_window_thumbnail_with_timeout(hwnd: HWND) -> Option<String> {
    let hwnd_val = hwnd.0 as usize;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = capture_window_thumbnail(HWND(hwnd_val as *mut std::ffi::c_void));
        let _ = tx.send(result);
    });
    let result: Option<String> = rx
        .recv_timeout(std::time::Duration::from_millis(500))
        .unwrap_or_default();
    result
}

pub(super) fn gather_window_infos() -> Result<Vec<serde_json::Value>, String> {
    let windows = windows_capture::window::Window::enumerate().map_err(|e| e.to_string())?;
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
            capture_window_thumbnail_with_timeout(HWND(hwnd_val as *mut std::ffi::c_void));
        let icon_data_url = window
            .process_id()
            .ok()
            .and_then(get_process_exe_path)
            .and_then(|path| extract_icon_data_url_from_exe(&path));
        let mut is_admin = false;
        if let Ok(pid) = window.process_id() {
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
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
pub(super) fn gather_window_metadata() -> Result<Vec<serde_json::Value>, String> {
    let windows = windows_capture::window::Window::enumerate().map_err(|e| e.to_string())?;
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
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
            if handle.is_err() {
                is_admin = true;
            } else if let Ok(h) = handle {
                unsafe {
                    let _ = CloseHandle(h);
                }
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
