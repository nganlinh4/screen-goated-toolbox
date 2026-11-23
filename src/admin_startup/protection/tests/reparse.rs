use super::{Fixture, open_component, reference_exists};
use crate::admin_startup::protection::acl::Kind;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::AsRawHandle as _;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
use windows::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

#[test]
fn an_owned_directory_junction_is_rejected_without_following_it() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    let junction = fixture.root.join("junction");
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir(&junction).unwrap();
    let target = std::fs::canonicalize(target).unwrap();
    let target = target.to_str().unwrap().strip_prefix(r"\\?\").unwrap();
    let substitute = format!(r"\??\{target}").encode_utf16().collect::<Vec<_>>();
    let print = target.encode_utf16().collect::<Vec<_>>();
    let path_bytes = (substitute.len() + print.len() + 2) * 2;
    let mut buffer = Vec::with_capacity(16 + path_bytes);
    buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buffer.extend(u16::try_from(8 + path_bytes).unwrap().to_le_bytes());
    buffer.extend(0_u16.to_le_bytes());
    buffer.extend(0_u16.to_le_bytes());
    buffer.extend(u16::try_from(substitute.len() * 2).unwrap().to_le_bytes());
    buffer.extend(
        u16::try_from((substitute.len() + 1) * 2)
            .unwrap()
            .to_le_bytes(),
    );
    buffer.extend(u16::try_from(print.len() * 2).unwrap().to_le_bytes());
    for unit in substitute
        .into_iter()
        .chain(Some(0))
        .chain(print)
        .chain(Some(0))
    {
        buffer.extend(unit.to_le_bytes());
    }
    let directory = OpenOptions::new()
        .write(true)
        .access_mode(FILE_GENERIC_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(&junction)
        .unwrap();
    let mut returned = 0;
    unsafe {
        DeviceIoControl(
            HANDLE(directory.as_raw_handle()),
            FSCTL_SET_REPARSE_POINT,
            Some(buffer.as_ptr().cast()),
            buffer.len() as u32,
            None,
            0,
            Some(&mut returned),
            None,
        )
    }
    .unwrap();
    drop(directory);
    let error = open_component(&junction, Kind::Ancestor).unwrap_err();
    assert!(error.to_string().contains("reparse point"), "{error:#}");
    for path in [
        junction.join("missing.exe"),
        junction.join("missing").join("candidate.exe"),
    ] {
        let error = reference_exists(&path).unwrap_err();
        assert!(error.to_string().contains("reparse point"), "{error:#}");
    }
    assert!(std::fs::read_dir(target).unwrap().next().is_none());
}
