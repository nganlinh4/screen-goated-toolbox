use std::fs;
use std::path::{Path, PathBuf};

use super::path_validation::{metadata_is_reparse, validate_file_name};

pub fn delete_recording_file(path: &str) -> Result<(), String> {
    let local = crate::paths::app_runtime_local_data_dir();
    delete_confined_file(
        Path::new(path),
        &[
            local.join("recordings"),
            local.join("composition-snapshots"),
        ],
    )
}

fn delete_confined_file(path: &Path, approved_roots: &[PathBuf]) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Managed recording is unavailable: {error}"))?;
    if !metadata.is_file() || metadata_is_reparse(&metadata) {
        return Err("Refusing to delete a non-regular recording file".to_string());
    }
    let resolved = fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve managed recording: {error}"))?;
    let approved = approved_roots.iter().any(|root| {
        fs::canonicalize(root)
            .ok()
            .is_some_and(|root| resolved.starts_with(&root) && resolved != root)
    });
    if !approved {
        return Err("Refusing to delete a file outside app-managed recordings".to_string());
    }
    fs::remove_file(&resolved)
        .map_err(|error| format!("Could not delete managed recording: {error}"))
}

/// Renames a media file in place, keeping its extension.
pub(super) fn rename_media_file(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let new_name = validate_file_name(new_name)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Media file is unavailable: {error}"))?;
    if !metadata.is_file() || metadata_is_reparse(&metadata) {
        return Err("Refusing to rename a non-regular media file".to_string());
    }
    let old_extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let new_extension = Path::new(&new_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if old_extension.is_empty() || !old_extension.eq_ignore_ascii_case(new_extension) {
        return Err("Renaming must preserve the media file extension".to_string());
    }
    let new_path = path.with_file_name(new_name);
    rename_without_replacing(path, &new_path)?;
    Ok(new_path)
}

/// `fs::rename` replaces an existing target, and an `exists()` pre-check both
/// races and rejects a case-only rename (the old file "exists" under the new
/// name on NTFS). Without MOVEFILE_REPLACE_EXISTING the OS does both right.
fn rename_without_replacing(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS};
    use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
    use windows::core::{HRESULT, PCWSTR};

    let wide = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() };
    let (from, to) = (wide(from), wide(to));
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .map_err(|error| {
        let exists = [ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS]
            .into_iter()
            .any(|code| error.code() == HRESULT::from_win32(code.0));
        if exists {
            "A file with that name already exists".to_string()
        } else {
            format!("Rename failed: {error}")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sgt-managed-recording-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn only_files_below_an_approved_root_can_be_deleted() {
        let root = fixture("confined");
        let recordings = root.join("recordings");
        let outside = root.join("exported.mp4");
        fs::create_dir_all(&recordings).unwrap();
        let managed = recordings.join("raw.mp4");
        fs::write(&managed, b"managed").unwrap();
        fs::write(&outside, b"user export").unwrap();

        delete_confined_file(&managed, std::slice::from_ref(&recordings)).unwrap();
        assert!(!managed.exists());
        assert!(delete_confined_file(&outside, &[recordings]).is_err());
        assert!(outside.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn renaming_can_change_only_the_letter_case() {
        let root = fixture("rename-case");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let clip = root.join("Clip.mp4");
        fs::write(&clip, b"clip").unwrap();

        let renamed = rename_media_file(&clip, "clip.mp4").unwrap();
        assert_eq!(renamed, root.join("clip.mp4"));
        let names: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["clip.mp4"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn renaming_never_replaces_another_file() {
        let root = fixture("rename-keep");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let clip = root.join("a.mp4");
        let other = root.join("b.mp4");
        fs::write(&clip, b"a").unwrap();
        fs::write(&other, b"b").unwrap();

        assert!(rename_media_file(&clip, "b.mp4").is_err());
        assert!(rename_media_file(&clip, "c.mov").is_err());
        assert_eq!(fs::read(&other).unwrap(), b"b");
        assert_eq!(fs::read(&clip).unwrap(), b"a");
        fs::remove_dir_all(root).unwrap();
    }
}
