use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo, GetFileInformationByHandle,
    SetFileInformationByHandle, VOLUME_NAME_GUID,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::overlay::computer_control::browser) enum Error {
    Unavailable,
    Invalid,
    Entropy,
}
impl Error {
    pub(in crate::overlay::computer_control::browser) fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "ERR_BROWSER_PAIRING_STORAGE",
            Self::Invalid => "ERR_BROWSER_PAIRING_KEY_INVALID",
            Self::Entropy => "ERR_BROWSER_PAIRING_ENTROPY",
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}
impl std::error::Error for Error {}

pub(super) fn load_or_make(path: &Path) -> Result<String, Error> {
    load_with(path, || {
        super::crypto::random_hex(24).map_err(|_| Error::Entropy)
    })
}

fn load_with(
    path: &Path,
    generate: impl FnOnce() -> Result<String, Error>,
) -> Result<String, Error> {
    let parent = path.parent().ok_or(Error::Unavailable)?;
    let directories = pin_directories(parent).map_err(|_| Error::Unavailable)?;
    if let Some(value) = read(path)? {
        return Ok(value);
    }
    remove_abandoned_scratch(parent);
    let value = generate()?;
    if !valid(&value) || publicly_derivable(&value) {
        return Err(Error::Entropy);
    }
    let suffix = super::crypto::random_hex(16).map_err(|_| Error::Entropy)?;
    let temporary = parent.join(format!("{SCRATCH_PREFIX}{suffix}{SCRATCH_SUFFIX}"));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(temporary)
        .map_err(|_| Error::Unavailable)?;
    let mut scratch = Scratch {
        file,
        published: false,
    };
    scratch
        .file
        .write_all(value.as_bytes())
        .and_then(|()| scratch.file.sync_all())
        .map_err(|_| Error::Unavailable)?;
    let result = publish(
        &scratch.file,
        directories.last().ok_or(Error::Unavailable)?,
        path,
    );
    if result.is_ok() {
        scratch.published = true;
        return Ok(value);
    }
    // A concurrent creator may have published first. Read only the committed
    // winner; never expose our unpublished candidate as the pairing identity.
    read(path)?.ok_or(Error::Unavailable)
}

const SCRATCH_PREFIX: &str = ".sgt-pairing-";
const SCRATCH_SUFFIX: &str = ".tmp";

/// Delete scratch files a crashed creator left behind. A live creator holds
/// its scratch open without sharing, so its file cannot be deleted here.
fn remove_abandoned_scratch(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_scratch = name
            .strip_prefix(SCRATCH_PREFIX)
            .and_then(|rest| rest.strip_suffix(SCRATCH_SUFFIX))
            .is_some_and(|id| {
                id.len() == 32
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
        if is_scratch && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Earlier builds filled bytes with this fixed pattern when secure randomness
/// failed. Such a key is public and must never authenticate an extension.
fn publicly_derivable(value: &str) -> bool {
    let fallback: String = (0..24u8)
        .map(|index| format!("{:02x}", index.wrapping_mul(31).wrapping_add(7)))
        .collect();
    value == fallback
}

fn valid(value: &str) -> bool {
    value.len() == 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read(path: &Path) -> Result<Option<String>, Error> {
    let deadline = Instant::now() + Duration::from_millis(250);
    let mut file = loop {
        match OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
        {
            Ok(file) => break file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error)
                if matches!(error.raw_os_error(), Some(32 | 33)) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(_) => return Err(Error::Unavailable),
        }
    };
    let metadata = file.metadata().map_err(|_| Error::Unavailable)?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }
        .map_err(|_| Error::Unavailable)?;
    if !metadata.is_file()
        || metadata.file_attributes() & 0x400 != 0
        || metadata.len() > 128
        || information.nNumberOfLinks != 1
    {
        return Err(Error::Invalid);
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(129)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Unavailable)?;
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Invalid)?
        .trim();
    if bytes.len() > 128 || !valid(value) || publicly_derivable(value) {
        return Err(Error::Invalid);
    }
    Ok(Some(value.to_owned()))
}

fn pin_directories(parent: &Path) -> std::io::Result<Vec<File>> {
    if !parent.is_absolute() {
        return Err(std::io::Error::other("pairing root is not absolute"));
    }
    let mut paths = parent.ancestors().collect::<Vec<_>>();
    paths.reverse();
    let mut directories = Vec::new();
    for path in paths {
        if !path.try_exists()? {
            match std::fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        let directory = OpenOptions::new()
            .read(true)
            .access_mode(FILE_READ_ATTRIBUTES.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let metadata = directory.metadata()?;
        if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
            return Err(std::io::Error::other("unsafe pairing directory"));
        }
        directories.push(directory);
    }
    Ok(directories)
}

struct Scratch {
    file: File,
    published: bool,
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.published {
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            let _ = unsafe {
                SetFileInformationByHandle(
                    HANDLE(self.file.as_raw_handle()),
                    FileDispositionInfo,
                    std::ptr::from_ref(&info).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            };
        }
    }
}

fn publish(file: &File, parent: &File, destination: &Path) -> Result<(), Error> {
    crate::overlay::computer_control::native_file::publish_new(
        file,
        parent,
        destination,
        VOLUME_NAME_GUID,
    )
    .map_err(|_| Error::Unavailable)
}

#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
