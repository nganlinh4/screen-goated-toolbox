use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
pub(super) fn refresh_cursor_settings() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, SystemParametersInfoW, HWND_BROADCAST, SMTO_ABORTIFHUNG,
        SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETCURSORS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        WM_SETTINGCHANGE,
    };

    unsafe {
        let flags = SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(SPIF_SENDCHANGE.0 | SPIF_UPDATEINIFILE.0);
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, flags);

        for area in [
            "Control Panel\\Cursors",
            "Software\\Microsoft\\Accessibility",
        ] {
            let wide: Vec<u16> = OsStr::new(area)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(wide.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                100,
                None,
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) fn write_accessibility_cursor_size(
    hkcu: &winreg::RegKey,
    cursor_size: u32,
) -> Result<(), String> {
    let (accessibility_key, _) = hkcu
        .create_subkey("Software\\Microsoft\\Accessibility")
        .map_err(|e| format!("Failed to open HKCU accessibility key: {}", e))?;
    accessibility_key
        .set_value("CursorSize", &cursor_size)
        .map_err(|e| format!("Failed writing accessibility cursor size: {}", e))
}

#[cfg(target_os = "windows")]
pub(super) fn set_system_cursor_from_file(
    path: &Path,
    cursor_id: windows::Win32::UI::WindowsAndMessaging::SYSTEM_CURSOR_ID,
    target_size: Option<u32>,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        LoadCursorFromFileW, LoadImageW, SetSystemCursor, IMAGE_CURSOR, LR_LOADFROMFILE,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let size_i32 = target_size
        .map(|value| i32::try_from(value).map_err(|_| format!("Invalid cursor size {}", value)))
        .transpose()?;

    let is_cur = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cur"));

    unsafe {
        let cursor = if is_cur {
            if let Some(size) = size_i32 {
                let handle = LoadImageW(
                    None,
                    PCWSTR(wide.as_ptr()),
                    IMAGE_CURSOR,
                    size,
                    size,
                    LR_LOADFROMFILE,
                )
                .map_err(|e| {
                    format!(
                        "Failed loading cursor {:?} with LoadImageW(size={}): {}",
                        path, size, e
                    )
                })?;
                windows::Win32::UI::WindowsAndMessaging::HCURSOR(handle.0)
            } else {
                LoadCursorFromFileW(PCWSTR(wide.as_ptr())).map_err(|e| {
                    format!(
                        "Failed loading cursor {:?} with LoadCursorFromFileW: {}",
                        path, e
                    )
                })?
            }
        } else {
            LoadCursorFromFileW(PCWSTR(wide.as_ptr())).map_err(|e| {
                format!(
                    "Failed loading cursor {:?} with LoadCursorFromFileW: {}",
                    path, e
                )
            })?
        };
        SetSystemCursor(cursor, cursor_id)
            .map_err(|e| format!("Failed setting system cursor {:?}: {}", path, e))?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn apply_standard_system_cursors(
    effective_files: &HashMap<String, PathBuf>,
    original_files: &HashMap<String, PathBuf>,
    target_size: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        OCR_APPSTARTING, OCR_CROSS, OCR_HAND, OCR_HELP, OCR_IBEAM, OCR_NO, OCR_NORMAL, OCR_SIZEALL,
        OCR_SIZENESW, OCR_SIZENS, OCR_SIZENWSE, OCR_SIZEWE, OCR_UP, OCR_WAIT,
    };

    let mapping = [
        ("working.ani", OCR_APPSTARTING),
        ("pointer.cur", OCR_NORMAL),
        ("precision.cur", OCR_CROSS),
        ("link.cur", OCR_HAND),
        ("help.cur", OCR_HELP),
        ("beam.cur", OCR_IBEAM),
        ("unavailable.cur", OCR_NO),
        ("move.cur", OCR_SIZEALL),
        ("dgn2.cur", OCR_SIZENESW),
        ("vert.cur", OCR_SIZENS),
        ("dgn1.cur", OCR_SIZENWSE),
        ("horz.cur", OCR_SIZEWE),
        ("alternate.cur", OCR_UP),
        ("busy.ani", OCR_WAIT),
    ];

    let mut applied_ids = HashSet::<u32>::new();
    for (file_name, cursor_id) in mapping {
        if !applied_ids.insert(cursor_id.0) {
            continue;
        }
        let original = original_files.get(file_name);
        let effective = effective_files.get(file_name);
        let path = if original
            .or(effective)
            .and_then(|p| p.extension().and_then(|ext| ext.to_str()))
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cur"))
        {
            original.or(effective)
        } else {
            effective.or(original)
        };
        let Some(path) = path else {
            continue;
        };
        let preferred_size = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cur"))
            .then_some(target_size);
        let _ = set_system_cursor_from_file(path, cursor_id, preferred_size);
    }
}
