#[cfg(windows)]
pub(crate) fn from_file(file: &std::fs::File) -> Result<String, String> {
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }
        .map_err(|_| "File identity is unavailable.".to_string())?;
    Ok(format!(
        "{:08x}:{:08x}{:08x}",
        information.dwVolumeSerialNumber, information.nFileIndexHigh, information.nFileIndexLow
    ))
}

#[cfg(not(windows))]
pub(crate) fn from_file(file: &std::fs::File) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt as _;

    let metadata = file
        .metadata()
        .map_err(|_| "File identity is unavailable.".to_string())?;
    Ok(format!("{:016x}:{:016x}", metadata.dev(), metadata.ino()))
}

pub(crate) fn from_path(path: &std::path::Path) -> Result<String, String> {
    let file =
        std::fs::File::open(path).map_err(|_| "File identity is unavailable.".to_string())?;
    from_file(&file)
}

pub(crate) fn valid(identity: &str) -> bool {
    identity.split_once(':').is_some_and(|(volume, file)| {
        matches!(volume.len(), 8 | 16)
            && file.len() == 16
            && volume.bytes().all(|byte| byte.is_ascii_hexdigit())
            && file.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
