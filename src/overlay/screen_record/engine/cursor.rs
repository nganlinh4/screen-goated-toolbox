use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem::zeroed;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP, ReleaseDC,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CURSORINFO, GetCursorInfo, GetIconInfo, ICONINFO, IDC_APPSTARTING, IDC_ARROW, IDC_CROSS,
    IDC_HAND, IDC_IBEAM, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_WAIT,
    LoadCursorW,
};

use super::types::{
    CURSOR_DEBUG_ENABLED, CURSOR_GRAB_LEARN_WINDOW_MS, CURSOR_SIGNATURE_CACHE,
    CUSTOM_GRAB_SIGNATURES, DEFAULT_GRAB_SIGNATURE, LAST_CURSOR_DEBUG,
    LAST_UNKNOWN_RELEASED_SIGNATURE, SYSTEM_CURSOR_HANDLES, SYSTEM_CURSOR_SIGNATURES,
    SystemCursorHandles,
};

pub(crate) fn get_cursor_type(is_clicked: bool) -> String {
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

pub(crate) fn load_system_cursor_handles() -> SystemCursorHandles {
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

pub(crate) fn load_system_cursor_signatures() -> HashMap<String, &'static str> {
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

pub(crate) fn load_grab_signatures() -> HashSet<String> {
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

pub(super) fn clear_last_unknown_released_signature() {
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

pub(crate) fn any_mouse_button_down() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_MBUTTON.0 as i32) as u16 & 0x8000) != 0
    }
}
