use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::*;

static ICON_CACHE: LazyLock<Mutex<HashMap<(u32, usize), Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn hicon_to_data_url(icon: HICON, destroy_icon: bool) -> Option<String> {
    unsafe {
        if icon.is_invalid() {
            return None;
        }

        let mut icon_info = ICONINFO::default();
        if GetIconInfo(icon, &mut icon_info).is_err() {
            if destroy_icon {
                let _ = DestroyIcon(icon);
            }
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
            if destroy_icon {
                let _ = DestroyIcon(icon);
            }
            return None;
        }

        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;
        if width == 0 || height == 0 {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            if destroy_icon {
                let _ = DestroyIcon(icon);
            }
            return None;
        }
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            if destroy_icon {
                let _ = DestroyIcon(icon);
            }
            return None;
        }
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DeleteObject(icon_info.hbmColor.into());
            if destroy_icon {
                let _ = DestroyIcon(icon);
            }
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
        if destroy_icon {
            let _ = DestroyIcon(icon);
        }

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

        hicon_to_data_url(large_icon, true)
    }
}

fn window_icon_handle(hwnd: HWND) -> Option<HICON> {
    unsafe {
        let mut result = 0usize;
        for icon_type in [ICON_BIG, ICON_SMALL2, ICON_SMALL] {
            if SendMessageTimeoutW(
                hwnd,
                WM_GETICON,
                WPARAM(icon_type as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                100,
                Some(&mut result),
            ) != LRESULT(0)
            {
                let icon = HICON(result as *mut std::ffi::c_void);
                if !icon.is_invalid() {
                    return Some(icon);
                }
            }
        }

        for class_index in [GCLP_HICON, GCLP_HICONSM] {
            let icon_ptr = GetClassLongPtrW(hwnd, class_index);
            let icon = HICON(icon_ptr as *mut std::ffi::c_void);
            if !icon.is_invalid() {
                return Some(icon);
            }
        }

        None
    }
}

fn extract_icon_data_url_from_window(hwnd: HWND) -> Option<String> {
    window_icon_handle(hwnd).and_then(|icon| hicon_to_data_url(icon, false))
}

fn encode_png_file_data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    use base64::Engine;
    let base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:image/png;base64,{base64}"))
}

fn package_root_from_exe_path(exe_path: &str) -> Option<PathBuf> {
    let mut current = Path::new(exe_path).parent();
    while let Some(path) = current {
        if path.join("AppxManifest.xml").is_file() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

fn attr_value(text: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = text.find(&needle)? + needle.len();
    let end = text[start..].find('"')?;
    Some(text[start..start + end].to_string())
}

fn app_manifest_block<'a>(manifest: &'a str, exe_name: &str) -> &'a str {
    let Some(exe_index) = manifest
        .to_ascii_lowercase()
        .find(&format!("executable=\"{}\"", exe_name.to_ascii_lowercase()))
    else {
        return manifest;
    };
    let start = manifest[..exe_index]
        .rfind("<Application")
        .or_else(|| manifest[..exe_index].rfind("<uap:Application"))
        .unwrap_or(0);
    let end = manifest[exe_index..]
        .find("</Application>")
        .or_else(|| manifest[exe_index..].find("</uap:Application>"))
        .map(|offset| exe_index + offset)
        .unwrap_or(manifest.len());
    &manifest[start..end]
}

fn package_logo_candidates(manifest_block: &str) -> Vec<String> {
    [
        "Square44x44Logo",
        "Square150x150Logo",
        "Square71x71Logo",
        "Square310x310Logo",
        "Logo",
    ]
    .iter()
    .filter_map(|name| attr_value(manifest_block, name))
    .collect()
}

fn appx_asset_score(path: &Path) -> i32 {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.contains("targetsize-48") {
        0
    } else if name.contains("targetsize-44") {
        1
    } else if name.contains("targetsize-32") {
        2
    } else if name.contains("scale-200") {
        10
    } else if name.contains("scale-100") {
        11
    } else {
        20
    }
}

fn resolve_appx_asset_path(package_root: &Path, manifest_asset: &str) -> Option<PathBuf> {
    let normalized = manifest_asset.replace(['\\', '/'], std::path::MAIN_SEPARATOR_STR);
    let exact_path = package_root.join(&normalized);
    if exact_path.is_file() {
        return Some(exact_path);
    }

    let asset_path = Path::new(&normalized);
    let parent = asset_path.parent().unwrap_or_else(|| Path::new(""));
    let dir = package_root.join(parent);
    let stem = asset_path.file_stem()?.to_str()?;
    let extension = asset_path.extension()?.to_str()?;
    let prefix = format!("{stem}.");
    let mut matches: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect();
    matches.sort_by_key(|path| appx_asset_score(path));
    matches.into_iter().next()
}

fn extract_icon_data_url_from_appx_manifest(exe_path: &str) -> Option<String> {
    let package_root = package_root_from_exe_path(exe_path)?;
    let manifest = std::fs::read_to_string(package_root.join("AppxManifest.xml")).ok()?;
    let exe_name = Path::new(exe_path).file_name()?.to_str()?;
    let block = app_manifest_block(&manifest, exe_name);

    for asset in package_logo_candidates(block) {
        if let Some(asset_path) = resolve_appx_asset_path(&package_root, &asset)
            && let Some(data_url) = encode_png_file_data_url(&asset_path)
        {
            return Some(data_url);
        }
    }

    None
}

pub fn get_app_icon_data_url(
    pid: u32,
    hwnd: HWND,
    window_exe_path: Option<&str>,
    capture_exe_path: Option<&str>,
    hosted_exe_paths: &[String],
) -> Option<String> {
    let cache_key = (pid, hwnd.0 as usize);
    {
        let cache = ICON_CACHE.lock().ok()?;
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let icon = extract_icon_data_url_from_window(hwnd)
        .or_else(|| capture_exe_path.and_then(extract_icon_data_url_from_appx_manifest))
        .or_else(|| {
            hosted_exe_paths
                .iter()
                .find_map(|path| extract_icon_data_url_from_appx_manifest(path))
        })
        .or_else(|| window_exe_path.and_then(extract_icon_data_url_from_appx_manifest))
        .or_else(|| capture_exe_path.and_then(extract_icon_data_url_from_exe))
        .or_else(|| {
            hosted_exe_paths
                .iter()
                .find_map(|path| extract_icon_data_url_from_exe(path))
        })
        .or_else(|| window_exe_path.and_then(extract_icon_data_url_from_exe));
    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.insert(cache_key, icon.clone());
    }
    icon
}
