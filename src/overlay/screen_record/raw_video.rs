use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock,
};
use windows::Win32::UI::Shell::DROPFILES;
use windows::core::{BOOL, PCWSTR};

const CLIPBOARD_RETRY_COUNT: usize = 5;
const CLIPBOARD_RETRY_DELAY_MS: u64 = 10;
const DROPEFFECT_COPY: u32 = 1;
const CLIPBOARD_FORMAT_CF_HDROP: u32 = 15;

static SAVED_RAW_VIDEO_CAPABILITIES: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Shared by every "save into a user folder" path (raw videos, subtitles).
pub(super) fn sanitize_dir_path(path: &str) -> Result<PathBuf, String> {
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

pub(super) fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
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

const RAW_VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi"];

/// The WebView may only export a video the recorder itself manages: a file in
/// the recordings directory with a video extension. Anything else (settings,
/// credentials, arbitrary user files) is refused.
fn ensure_recorder_owned_video(path: &str) -> Result<PathBuf, String> {
    let source = ensure_source_video(path)?;
    let canonical =
        fs::canonicalize(&source).map_err(|error| format!("Resolve raw video failed: {error}"))?;
    let recordings = fs::canonicalize(super::ipc::media_server::recordings_dir())
        .map_err(|error| format!("Resolve recordings directory failed: {error}"))?;
    let is_video = canonical
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| RAW_VIDEO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
    if !canonical.starts_with(&recordings) || !is_video {
        return Err("Only recorder-managed videos can be saved".to_string());
    }
    Ok(canonical)
}

pub fn save_raw_video_copy(source_path: &str, target_dir: &str) -> Result<String, String> {
    let source = ensure_recorder_owned_video(source_path)?;
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

    if let Ok(canonical) = fs::canonicalize(&destination) {
        SAVED_RAW_VIDEO_CAPABILITIES
            .lock()
            .unwrap()
            .insert(canonical);
    }

    Ok(destination.to_string_lossy().to_string())
}

pub fn save_composition_snapshot_copy(source_path: &str) -> Result<String, String> {
    let source = ensure_source_video(source_path)?;
    let dir = crate::paths::app_runtime_local_data_dir().join("composition-snapshots");
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create composition snapshot dir: {}", e))?;
    let source_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("snapshot.mp4");
    let destination = unique_destination(&dir, source_name);

    fs::copy(&source, &destination).map_err(|e| {
        format!(
            "Failed to copy composition snapshot {} -> {}: {}",
            source.display(),
            destination.display(),
            e
        )
    })?;

    Ok(destination.to_string_lossy().to_string())
}

pub fn move_saved_raw_video(current_path: &str, target_dir: &str) -> Result<String, String> {
    let current = ensure_source_video(current_path)?;
    let canonical_current = fs::canonicalize(&current)
        .map_err(|error| format!("Resolve saved raw video failed: {error}"))?;
    if !SAVED_RAW_VIDEO_CAPABILITIES
        .lock()
        .unwrap()
        .contains(&canonical_current)
    {
        return Err("Raw video was not saved by this recorder session".to_string());
    }
    let dir = sanitize_dir_path(target_dir)?;
    // unique_destination always sees the file itself in its own folder, so a
    // same-folder "move" would otherwise rename it to "name (1)".
    if fs::canonicalize(&dir).ok().as_deref() == canonical_current.parent() {
        return Ok(current.to_string_lossy().to_string());
    }
    let current_name = current
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("recording.mp4");
    let destination = unique_destination(&dir, current_name);

    if fs::rename(&current, &destination).is_err() {
        // Fallback for cross-volume moves.
        fs::copy(&current, &destination).map_err(|e| {
            format!(
                "Failed to move raw video {} -> {}: {}",
                current.display(),
                destination.display(),
                e
            )
        })?;
        if let Err(error) = fs::remove_file(&current) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Copied file but failed to remove original {}: {}",
                current.display(),
                error
            ));
        }
    }
    let canonical_destination = fs::canonicalize(&destination)
        .map_err(|error| format!("Resolve moved raw video failed: {error}"))?;
    let mut capabilities = SAVED_RAW_VIDEO_CAPABILITIES.lock().unwrap();
    capabilities.remove(&canonical_current);
    capabilities.insert(canonical_destination);
    Ok(destination.to_string_lossy().to_string())
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
        if preferred_format != 0
            && let Ok(h_effect) =
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
            if result.is_ok() {
                crate::overlay::auto_copy_badge::show_auto_copy_badge_media_file(&path_string);
            }
            return result;
        }

        if attempt + 1 < CLIPBOARD_RETRY_COUNT {
            std::thread::sleep(Duration::from_millis(CLIPBOARD_RETRY_DELAY_MS));
        }
    }

    Err("Failed to open clipboard".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_video_copy_only_accepts_recorder_managed_videos() {
        let recordings = super::super::ipc::media_server::recordings_dir();
        fs::create_dir_all(&recordings).unwrap();
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let tag = format!("{}-{}", std::process::id(), u64::from_ne_bytes(random));
        let video = recordings.join(format!("recording_test-{tag}.mp4"));
        let not_video = recordings.join(format!("notes-{tag}.json"));
        let outside = std::env::temp_dir().join(format!("config-{tag}.mp4"));
        for path in [&video, &not_video, &outside] {
            fs::write(path, b"data").unwrap();
        }
        let target = std::env::temp_dir().join(format!("sgt-raw-copy-{tag}"));

        assert!(
            save_raw_video_copy(&outside.to_string_lossy(), &target.to_string_lossy()).is_err()
        );
        assert!(
            save_raw_video_copy(&not_video.to_string_lossy(), &target.to_string_lossy()).is_err()
        );
        let saved =
            save_raw_video_copy(&video.to_string_lossy(), &target.to_string_lossy()).unwrap();
        assert!(Path::new(&saved).is_file());

        for path in [&video, &not_video, &outside] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir_all(target).unwrap();
    }
    #[test]
    fn moving_a_saved_video_into_its_own_folder_keeps_its_name() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "sgt-raw-move-{}-{}",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        fs::create_dir_all(&dir).unwrap();
        let saved = dir.join("Recording.mp4");
        fs::write(&saved, b"data").unwrap();
        SAVED_RAW_VIDEO_CAPABILITIES
            .lock()
            .unwrap()
            .insert(fs::canonicalize(&saved).unwrap());

        let moved = move_saved_raw_video(&saved.to_string_lossy(), &dir.to_string_lossy()).unwrap();
        assert_eq!(Path::new(&moved), saved.as_path());
        assert!(saved.is_file());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
