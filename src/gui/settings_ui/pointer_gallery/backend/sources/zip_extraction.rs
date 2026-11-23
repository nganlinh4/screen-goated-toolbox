use std::fs::{File, OpenOptions};
use std::io::{self, Read as _, Seek, Write as _};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileDispositionInfo, GetFinalPathNameByHandleW, SetFileInformationByHandle, VOLUME_NAME_GUID,
};

const MAX_CURSOR_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn extract<R: io::Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    destination: &Path,
) -> Result<(), String> {
    let mut entry = archive
        .by_index(index)
        .map_err(|error| format!("Failed extracting zip entry {index}: {error}"))?;
    let expected = entry.size();
    if expected == 0 || expected > MAX_CURSOR_BYTES {
        return Err("Extracted cursor file has an invalid size".to_string());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "Cursor destination has no parent".to_string())?;
    let directories = lock_directories(parent)?;
    let parent_handle = directories
        .last()
        .ok_or_else(|| "Cursor destination has no directory".to_string())?;

    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| format!("Cursor staging identity: {error}"))?;
    let staging = parent.join(format!(
        ".sgt-cursor-{:032x}.tmp",
        u128::from_le_bytes(nonce)
    ));
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(staging)
        .map_err(|error| format!("Could not create cursor staging file: {error}"))?;
    let mut scratch = Scratch {
        file: output,
        published: false,
    };
    let copied = io::copy(
        &mut entry.by_ref().take(MAX_CURSOR_BYTES + 1),
        &mut scratch.file,
    )
    .map_err(|error| format!("Could not extract cursor file: {error}"))?;
    if copied != expected || copied > MAX_CURSOR_BYTES {
        return Err("Extracted cursor file has an invalid size".to_string());
    }
    scratch
        .file
        .flush()
        .map_err(|error| format!("Could not flush cursor file: {error}"))?;
    publish(&scratch.file, parent_handle, destination)?;
    scratch.published = true;
    Ok(())
}

struct Scratch {
    file: File,
    published: bool,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.published {
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            let _ = unsafe {
                SetFileInformationByHandle(
                    HANDLE(self.file.as_raw_handle()),
                    FileDispositionInfo,
                    std::ptr::from_ref(&disposition).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            };
        }
    }
}

fn lock_directories(path: &Path) -> Result<Vec<File>, String> {
    if !path.is_absolute() {
        return Err("Cursor directory must be absolute".to_string());
    }
    let mut ancestors = path.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    let mut directories = Vec::with_capacity(ancestors.len());
    for ancestor in ancestors {
        let file = OpenOptions::new()
            .read(true)
            .access_mode(FILE_READ_ATTRIBUTES.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(ancestor)
            .map_err(|error| format!("Could not lock cursor directory: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("Could not inspect cursor directory: {error}"))?;
        if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
            return Err("Cursor directory is unsafe".to_string());
        }
        directories.push(file);
    }
    Ok(directories)
}

fn publish(file: &File, parent: &File, destination: &Path) -> Result<(), String> {
    let leaf = destination
        .file_name()
        .ok_or_else(|| "Cursor destination has no filename".to_string())?
        .encode_wide()
        .collect::<Vec<_>>();
    // Use the pinned directory's volume identity while its entire ancestor
    // chain is held against replacement. Win32 rename requires an absolute name.
    let mut name = vec![0_u16; 32768];
    let length = unsafe {
        GetFinalPathNameByHandleW(HANDLE(parent.as_raw_handle()), &mut name, VOLUME_NAME_GUID)
    } as usize;
    if length == 0 || length >= name.len() {
        return Err("Could not resolve the pinned cursor directory".to_string());
    }
    name.truncate(length);
    if name.last() != Some(&u16::from(b'\\')) {
        name.push(u16::from(b'\\'));
    }
    name.extend(leaf);
    crate::verified_scratch::rename_file_by_handle(file, &name, false)
        .map_err(|error| format!("Could not publish cursor file: {error}"))?;
    Ok(())
}

#[cfg(test)]
#[path = "zip_extraction_tests.rs"]
mod tests;
