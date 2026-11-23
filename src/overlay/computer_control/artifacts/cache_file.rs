//! Handle-owned cache files; releasing the last lease removes only that file.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_DELETE_ON_CLOSE,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub(super) struct CacheFile {
    file: Mutex<File>,
    _directories: Vec<File>,
}

impl CacheFile {
    pub(super) fn create(path: &Path, text: &str) -> Result<Self> {
        let directories = pin_directories(path.parent().context("artifact path has no parent")?)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .context("create owned artifact cache file")?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        Ok(Self {
            file: Mutex::new(file),
            _directories: directories,
        })
    }

    pub(super) fn read(&self, maximum: u64) -> Result<String> {
        let mut file = self.file.lock().unwrap_or_else(|error| error.into_inner());
        file.seek(SeekFrom::Start(0))?;
        read_bounded(&mut file, maximum)
    }
}

pub(super) fn read_path(path: &Path, maximum: u64) -> Result<String> {
    // Deny concurrent writers/deletion while this exact opened file is read.
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)?;
    read_bounded(&mut file, maximum)
}

fn read_bounded(file: &mut File, maximum: u64) -> Result<String> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        bail!("artifact source is not a regular file");
    }
    if metadata.len() > maximum {
        bail!("artifact source exceeds the byte limit");
    }
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        bail!("artifact source exceeds the byte limit");
    }
    String::from_utf8(bytes).context("artifact source is not UTF-8 text")
}

fn pin_directories(parent: &Path) -> Result<Vec<File>> {
    if !parent.is_absolute() {
        bail!("artifact cache root must be absolute");
    }
    let mut paths = parent.ancestors().collect::<Vec<_>>();
    paths.reverse();
    let mut directories = Vec::new();
    for path in paths {
        if !path.try_exists()? {
            match std::fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        let directory = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let metadata = directory.metadata()?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            bail!("artifact cache parent is not an ordinary directory");
        }
        directories.push(directory);
    }
    Ok(directories)
}
