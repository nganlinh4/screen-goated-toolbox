use anyhow::{Result, ensure};
use std::path::Path;

pub(super) fn local_absolute(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    if !path.is_absolute() {
        return false;
    }
    matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        || matches!(prefix.kind(), Prefix::Verbatim(value) if value.to_str().is_some_and(|value| value.starts_with("Volume{") && value.ends_with('}'))))
}

pub(super) fn absolute_startup_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    local_absolute(path)
        || (path.is_absolute()
            && matches!(path.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(),
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share)
                if !server.is_empty() && !share.is_empty())))
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    if !absolute_startup_path(left) || !absolute_startup_path(right) {
        return false;
    }
    let left_units = left.as_os_str().encode_wide().collect::<Vec<_>>();
    let right_units = right.as_os_str().encode_wide().collect::<Vec<_>>();
    if unsafe { CompareStringOrdinal(&left_units, &right_units, true) == CSTR_EQUAL } {
        return true;
    }
    match (
        crate::verbatim_path::ordinary_namespace(left, left_units),
        crate::verbatim_path::ordinary_namespace(right, right_units),
    ) {
        (Some(left), Some(right)) => unsafe {
            CompareStringOrdinal(&left, &right, true) == CSTR_EQUAL
        },
        _ => false,
    }
}

pub(super) fn same_executable(left: &Path, right: &Path) -> bool {
    if !local_absolute(left) || !local_absolute(right) {
        return false;
    }
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let identity = |path: &Path| -> Result<_> {
        ensure!(
            local_absolute(path),
            "A local absolute executable is required."
        );
        let file = std::fs::File::open(path)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }?;
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    };
    match (identity(left), identity(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
