use std::path::Path;

#[cfg(windows)]
pub(super) fn create_locked(path: &Path) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    };

    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())
}

#[cfg(not(windows))]
pub(super) fn create_locked(path: &Path) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())
}

#[cfg(windows)]
pub(super) fn open_locked(path: &Path, write: bool, delete: bool) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    };

    let mut access = FILE_GENERIC_READ.0;
    if write {
        access |= FILE_GENERIC_WRITE.0;
    }
    if delete {
        access |= DELETE.0;
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(write)
        .access_mode(access)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())
}

#[cfg(not(windows))]
pub(super) fn open_locked(
    path: &Path,
    write: bool,
    _delete: bool,
) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(write)
        .open(path)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())
}

#[cfg(windows)]
pub(super) fn identity(file: &std::fs::File) -> Result<String, String> {
    crate::overlay::creation_file_identity::from_file(file)
        .map_err(|_| "Creation publication receipt identity is unavailable.".to_string())
}

#[cfg(not(windows))]
pub(super) fn identity(file: &std::fs::File) -> Result<String, String> {
    crate::overlay::creation_file_identity::from_file(file)
        .map_err(|_| "Creation publication receipt identity is unavailable.".to_string())
}

#[cfg(windows)]
pub(super) fn rename_no_replace(
    file: &std::fs::File,
    _source: &Path,
    destination: &Path,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };

    let destination = destination.as_os_str().encode_wide().collect::<Vec<_>>();
    let size = std::mem::size_of::<FILE_RENAME_INFO>()
        .saturating_add(destination.len().saturating_sub(1) * std::mem::size_of::<u16>());
    let mut buffer = vec![0_u8; size];
    let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*information).Anonymous.ReplaceIfExists = false;
        (*information).RootDirectory = HANDLE::default();
        (*information).FileNameLength =
            u32::try_from(destination.len() * std::mem::size_of::<u16>())
                .map_err(|_| "Creation output destination is invalid.".to_string())?;
        std::ptr::copy_nonoverlapping(
            destination.as_ptr(),
            std::ptr::addr_of_mut!((*information).FileName).cast::<u16>(),
            destination.len(),
        );
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            buffer.as_ptr().cast(),
            u32::try_from(buffer.len())
                .map_err(|_| "Creation output destination is invalid.".to_string())?,
        )
    }
    .map_err(|_| "Creation output destination is already in use.".to_string())
}

#[cfg(not(windows))]
pub(super) fn rename_no_replace(
    _file: &std::fs::File,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    std::fs::hard_link(source, destination)
        .map_err(|_| "Creation output destination is already in use.".to_string())?;
    std::fs::remove_file(source)
        .map_err(|_| "Creation publication receipt could not be committed.".to_string())
}

#[cfg(windows)]
pub(super) fn delete(file: &std::fs::File, _path: &Path) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };

    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(|_| "Creation publication cleanup could not finish.".to_string())
}

#[cfg(not(windows))]
pub(super) fn delete(_file: &std::fs::File, path: &Path) -> Result<(), String> {
    std::fs::remove_file(path)
        .map_err(|_| "Creation publication cleanup could not finish.".to_string())
}
