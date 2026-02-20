use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{HANDLE, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::UI::Shell::DROPFILES;

const CLIPBOARD_RETRY_COUNT: usize = 5;
const CLIPBOARD_RETRY_DELAY_MS: u64 = 10;
const DROPEFFECT_COPY: u32 = 1;
const CLIPBOARD_FORMAT_CF_HDROP: u32 = 15;

fn sanitize_dir_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Target directory is empty".to_string());
    }

    let dir = PathBuf::from(trimmed);
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create target directory {}: {}", dir.display(), e))?;
    }
    if !dir.is_dir() {
        return Err(format!("Target path is not a directory: {}", dir.display()));
    }
    Ok(dir)
}

fn ensure_source_video(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Source path is empty".to_string());
    }

    let source = PathBuf::from(trimmed);
    if !source.exists() {
        return Err(format!("Source file does not exist: {}", source.display()));
    }
    if !source.is_file() {
        return Err(format!("Source path is not a file: {}", source.display()));
    }
    Ok(source)
}

fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let base = Path::new(file_name);
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("recording");
    let ext = base.extension().and_then(|e| e.to_str()).unwrap_or("");

    let mut candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    for idx in 1..10_000 {
        let next_name = if ext.is_empty() {
            format!("{} ({})", stem, idx)
        } else {
            format!("{} ({}).{}", stem, idx, ext)
        };
        candidate = dir.join(next_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    // Extremely unlikely fallback.
    if ext.is_empty() {
        dir.join(format!("{}-copy", stem))
    } else {
        dir.join(format!("{}-copy.{}", stem, ext))
    }
}

pub fn save_raw_video_copy(source_path: &str, target_dir: &str) -> Result<String, String> {
    let source = ensure_source_video(source_path)?;
    let dir = sanitize_dir_path(target_dir)?;
    let source_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("recording.mp4");
    let destination = unique_destination(&dir, source_name);

    fs::copy(&source, &destination).map_err(|e| {
        format!(
            "Failed to copy raw video {} -> {}: {}",
            source.display(),
            destination.display(),
            e
        )
    })?;

    Ok(destination.to_string_lossy().to_string())
}

pub fn move_saved_raw_video(current_path: &str, target_dir: &str) -> Result<String, String> {
    let current = ensure_source_video(current_path)?;
    let dir = sanitize_dir_path(target_dir)?;
    let current_name = current
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("recording.mp4");
    let destination = unique_destination(&dir, current_name);

    if destination == current {
        return Ok(destination.to_string_lossy().to_string());
    }

    match fs::rename(&current, &destination) {
        Ok(_) => Ok(destination.to_string_lossy().to_string()),
        Err(_) => {
            // Fallback for cross-volume moves.
            fs::copy(&current, &destination).map_err(|e| {
                format!(
                    "Failed to move raw video {} -> {}: {}",
                    current.display(),
                    destination.display(),
                    e
                )
            })?;
            fs::remove_file(&current).map_err(|e| {
                format!(
                    "Copied file but failed to remove original {}: {}",
                    current.display(),
                    e
                )
            })?;
            Ok(destination.to_string_lossy().to_string())
        }
    }
}

fn to_wide_z(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_clipboard_file_drop(path: &str) -> Result<(), String> {
    let mut file_list = path.encode_utf16().collect::<Vec<u16>>();
    file_list.push(0);
    file_list.push(0); // double-null terminated string list

    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let list_size = file_list.len() * std::mem::size_of::<u16>();
    let total_size = dropfiles_size + list_size;

    unsafe {
        let h_drop =
            GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size).map_err(|e| e.to_string())?;
        let drop_ptr = GlobalLock(h_drop) as *mut u8;
        if drop_ptr.is_null() {
            return Err("Failed to lock clipboard memory for file drop".to_string());
        }

        let dropfiles = DROPFILES {
            pFiles: dropfiles_size as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: BOOL(0),
            fWide: BOOL(1),
        };
        std::ptr::write(drop_ptr as *mut DROPFILES, dropfiles);
        std::ptr::copy_nonoverlapping(
            file_list.as_ptr() as *const u8,
            drop_ptr.add(dropfiles_size),
            list_size,
        );

        let _ = GlobalUnlock(h_drop);

        if SetClipboardData(CLIPBOARD_FORMAT_CF_HDROP, Some(HANDLE(h_drop.0 as *mut _))).is_err() {
            return Err("Failed to set CF_HDROP clipboard data".to_string());
        }

        // Hint paste targets that this is a COPY operation.
        let preferred_format_name = to_wide_z("Preferred DropEffect");
        let preferred_format = RegisterClipboardFormatW(PCWSTR(preferred_format_name.as_ptr()));
        if preferred_format != 0 {
            if let Ok(h_effect) =
                GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, std::mem::size_of::<u32>())
            {
                let effect_ptr = GlobalLock(h_effect) as *mut u32;
                if !effect_ptr.is_null() {
                    *effect_ptr = DROPEFFECT_COPY;
                    let _ = GlobalUnlock(h_effect);
                    let _ = SetClipboardData(preferred_format, Some(HANDLE(h_effect.0 as *mut _)));
                }
            }
        }
    }

    Ok(())
}

pub fn copy_video_file_to_clipboard(file_path: &str) -> Result<(), String> {
    let source = ensure_source_video(file_path)?;
    let absolute = source.canonicalize().unwrap_or(source);
    let path_string = absolute.to_string_lossy().to_string();

    for attempt in 0..CLIPBOARD_RETRY_COUNT {
        if unsafe { OpenClipboard(None) }.is_ok() {
            let _ = unsafe { EmptyClipboard() };
            let result = set_clipboard_file_drop(&path_string);
            let _ = unsafe { CloseClipboard() };
            return result;
        }

        if attempt + 1 < CLIPBOARD_RETRY_COUNT {
            std::thread::sleep(Duration::from_millis(CLIPBOARD_RETRY_DELAY_MS));
        }
    }

    Err("Failed to open clipboard".to_string())
}
