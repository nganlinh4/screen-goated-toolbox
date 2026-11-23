//! Reset the app-owned WebView profile without traversing reparse points.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileDispositionInfo, SetFileInformationByHandle,
};

const MAX_ENTRIES: usize = 250_000;
const MAX_DEPTH: usize = 128;

pub(crate) fn clear_webview_permissions() -> io::Result<()> {
    if std::env::var_os("SGT_CREATION_WEBVIEW2_DATA_DIR").is_some() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "external WebView profile cannot be reset automatically",
        ));
    }
    let path = crate::paths::app_sgt_dir().join("webview_data");
    let mut last = None;
    for delay in [0, 100, 500] {
        if delay != 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        match clear_tree(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => return Err(error),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("WebView data reset failed")))
}

// No delete sharing pins each component against rename or replacement while
// its path is used. OPEN_REPARSE_POINT makes inspection non-following.
fn open_entry(path: &Path, delete: bool) -> io::Result<File> {
    let access = FILE_READ_ATTRIBUTES.0 | if delete { DELETE.0 } else { 0 };
    let file = OpenOptions::new()
        .read(true)
        .access_mode(access)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WebView reset path contains a reparse point",
        ));
    }
    Ok(file)
}

fn guard_ancestors(parent: &Path) -> io::Result<Option<Vec<File>>> {
    if !parent.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WebView reset path is not absolute",
        ));
    }
    let mut current = PathBuf::new();
    let mut guards = Vec::new();
    let mut seen_root = false;
    for component in parent.components() {
        if matches!(component, Component::CurDir | Component::ParentDir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WebView reset path is not normalized",
            ));
        }
        current.push(component);
        if matches!(component, Component::RootDir) {
            seen_root = true;
        }
        if !seen_root {
            continue;
        }
        match open_entry(&current, false) {
            Ok(directory) if directory.metadata()?.is_dir() => guards.push(directory),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "WebView reset ancestor is not a directory",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    Ok(Some(guards))
}

fn mark_for_deletion(file: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: file remains open and disposition has the required layout and size.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(io::Error::from)
}

fn remove_opened(path: &Path, file: File, entries: &mut usize, depth: usize) -> io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WebView reset tree exceeds safety limits",
        ));
    }
    let metadata = file.metadata()?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            *entries = entries
                .checked_add(1)
                .ok_or_else(|| io::Error::other("WebView reset entry count overflow"))?;
            if *entries > MAX_ENTRIES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "WebView reset tree exceeds safety limits",
                ));
            }
            let child_path = entry.path();
            let child = open_entry(&child_path, true)?;
            remove_opened(&child_path, child, entries, depth + 1)?;
        }
    } else if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WebView reset tree contains an unsupported entry",
        ));
    }
    mark_for_deletion(&file)
}

fn clear_tree(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "WebView reset has no parent"))?;
    let Some(_ancestors) = guard_ancestors(parent)? else {
        return Ok(());
    };
    let root = match open_entry(path, true) {
        Ok(root) => root,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !root.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WebView reset root is not a directory",
        ));
    }
    remove_opened(path, root, &mut 0, 0)
}

#[cfg(test)]
#[path = "webview_reset_tests.rs"]
mod tests;
