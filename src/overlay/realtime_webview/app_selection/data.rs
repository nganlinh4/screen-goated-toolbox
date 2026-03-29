use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::BOOL;

lazy_static::lazy_static! {
    static ref ICON_CACHE: Mutex<HashMap<u32, Option<String>>> = Mutex::new(HashMap::new());
}

#[derive(Clone, Debug)]
pub struct AudioAppCandidate {
    pub pid: u32,
    pub display_name: String,
    pub process_name: String,
    pub icon_data_url: Option<String>,
    pub window_hwnd: usize,
    pub width: u32,
    pub height: u32,
}

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

        let mut bitmap = BITMAP::default();
        if GetObjectW(
            icon_info.hbmColor.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut std::ffi::c_void),
        ) == 0
        {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            let _ = DestroyIcon(large_icon);
            return None;
        }

        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;
        if width == 0 || height == 0 {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            let _ = DestroyIcon(large_icon);
            return None;
        }
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            let _ = DestroyIcon(large_icon);
            return None;
        }
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            let _ = DestroyIcon(large_icon);
            return None;
        }

        let bitmap_info = BITMAPINFO {
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
            &bitmap_info as *const _ as *mut _,
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
        for index in (0..pixels.len()).step_by(4) {
            pixels.swap(index, index + 2);
            if pixels[index + 3] != 0 {
                has_alpha = true;
            }
        }

        if !has_alpha {
            for index in (3..pixels.len()).step_by(4) {
                pixels[index] = 255;
            }
        }

        let rgba_image = image::RgbaImage::from_raw(width, height, pixels)?;
        let mut png_data = Vec::new();
        if rgba_image
            .write_to(
                &mut std::io::Cursor::new(&mut png_data),
                image::ImageFormat::Png,
            )
            .is_err()
        {
            return None;
        }

        use base64::Engine;
        let base64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
        Some(format!("data:image/png;base64,{base64}"))
    }
}

fn get_app_icon_data_url(pid: u32, exe_path: Option<&str>) -> Option<String> {
    {
        let cache = ICON_CACHE.lock().ok()?;
        if let Some(cached) = cache.get(&pid) {
            return cached.clone();
        }
    }

    let icon = exe_path.and_then(extract_icon_data_url_from_exe);
    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.insert(pid, icon.clone());
    }
    icon
}

fn get_window_size(hwnd: HWND) -> (u32, u32) {
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

        (
            (rect.right - rect.left).max(1) as u32,
            (rect.bottom - rect.top).max(1) as u32,
        )
    }
}

pub fn enumerate_audio_app_candidates() -> Vec<AudioAppCandidate> {
    let mut apps: Vec<AudioAppCandidate> = Vec::new();
    let mut seen_pids: HashSet<u32> = HashSet::new();

    unsafe {
        let mut callback_data = (&mut apps, &mut seen_pids);

        extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            unsafe {
                if !IsWindowVisible(hwnd).as_bool() {
                    return BOOL(1);
                }

                let mut title_buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut title_buf);
                if len == 0 {
                    return BOOL(1);
                }

                let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                if title.is_empty() || title == "Program Manager" || title == "Settings" {
                    return BOOL(1);
                }

                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == 0 || pid == std::process::id() {
                    return BOOL(1);
                }

                let data =
                    &mut *(lparam.0 as *mut (&mut Vec<AudioAppCandidate>, &mut HashSet<u32>));
                let (apps, seen_pids) = data;
                if seen_pids.contains(&pid) {
                    return BOOL(1);
                }
                seen_pids.insert(pid);

                let exe_path = get_process_exe_path(pid);
                let process_name = exe_path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_stem())
                    .and_then(|stem| stem.to_str())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("PID {pid}"));
                let icon_data_url = get_app_icon_data_url(pid, exe_path.as_deref());
                let (width, height) = get_window_size(hwnd);

                apps.push(AudioAppCandidate {
                    pid,
                    display_name: title,
                    process_name,
                    icon_data_url,
                    window_hwnd: hwnd.0 as usize,
                    width,
                    height,
                });

                BOOL(1)
            }
        }

        let _ = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut callback_data as *mut _ as isize),
        );
    }

    apps.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
    });
    apps
}

pub fn enumerate_audio_apps() -> Vec<(u32, String)> {
    enumerate_audio_app_candidates()
        .into_iter()
        .map(|app| (app.pid, app.display_name))
        .collect()
}
