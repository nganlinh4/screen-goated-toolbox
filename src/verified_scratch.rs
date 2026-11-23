use std::fs::File;
use std::os::windows::io::AsRawHandle as _;

use windows::Win32::Foundation::{E_INVALIDARG, HANDLE};
use windows::Win32::Storage::FileSystem::{
    FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
};

/// Rename an owned handle using a terminated, aligned Win32 information buffer.
/// Directory ownership and replacement policy remain the caller's responsibility.
pub(crate) fn rename_file_by_handle(
    file: &File,
    name: &[u16],
    replace: bool,
) -> windows::core::Result<()> {
    if name.is_empty() || name.len() >= 32768 || name.contains(&0) {
        return Err(windows::core::Error::from_hresult(E_INVALIDARG));
    }
    let name_bytes = std::mem::size_of_val(name);
    // Win32 consumes a terminated name even though FileNameLength excludes NUL.
    // Reserve that code unit explicitly, including at every alignment boundary.
    let size = (std::mem::offset_of!(FILE_RENAME_INFO, FileName) + name_bytes + 2)
        .max(std::mem::size_of::<FILE_RENAME_INFO>());
    let mut buffer = vec![0_u64; size.div_ceil(std::mem::size_of::<u64>())];
    unsafe {
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = HANDLE::default();
        (*info).FileNameLength = name_bytes as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            buffer.as_ptr().cast(),
            size as u32,
        )
    }
}

#[cfg(test)]
#[path = "verified_scratch_tests.rs"]
mod tests;
