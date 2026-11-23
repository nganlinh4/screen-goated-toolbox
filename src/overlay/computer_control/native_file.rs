//! Native file identity and handle-bound, create-only namespace publication.

use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use anyhow::{Result, ensure};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_ID_INFO, FileIdInfo, GETFINALPATHNAMEBYHANDLE_FLAGS, GetFileInformationByHandleEx,
    GetFinalPathNameByHandleW,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub(super) volume: u64,
    pub(super) id: [u8; 16],
}

impl Identity {
    pub(super) fn read(file: &File) -> Result<Self> {
        let mut info = FILE_ID_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        }?;
        Ok(Self {
            volume: info.VolumeSerialNumber,
            id: info.FileId.Identifier,
        })
    }
}

pub(super) fn publish_new(
    file: &File,
    parent: &File,
    destination: &Path,
    volume: GETFINALPATHNAMEBYHANDLE_FLAGS,
) -> Result<()> {
    let mut name = vec![0_u16; 32768];
    let length =
        unsafe { GetFinalPathNameByHandleW(HANDLE(parent.as_raw_handle()), &mut name, volume) }
            as usize;
    ensure!(
        length != 0 && length < name.len(),
        "could not resolve publication parent"
    );
    name.truncate(length);
    if name.last() != Some(&u16::from(b'\\')) {
        name.push(u16::from(b'\\'));
    }
    name.extend(
        destination
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("missing destination name"))?
            .encode_wide(),
    );
    crate::verified_scratch::rename_file_by_handle(file, &name, false)?;
    Ok(())
}
