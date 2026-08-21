use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

use crate::component_registry::receipt::is_reparse_point;

pub(super) struct BoundedEntries {
    pub(super) paths: Vec<PathBuf>,
    pub(super) overflowed: bool,
}

pub(super) fn bounded_entries(parent: &Path, maximum: usize) -> Result<BoundedEntries> {
    let metadata = match std::fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BoundedEntries {
                paths: Vec::new(),
                overflowed: false,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("external tool recovery directory is unsafe");
    }
    let mut paths = Vec::new();
    let mut overflowed = false;
    for entry in std::fs::read_dir(parent)? {
        if paths.len() == maximum {
            overflowed = true;
            break;
        }
        paths.push(entry?.path());
    }
    Ok(BoundedEntries { paths, overflowed })
}

pub(super) fn collect_remaining(
    root: &Path,
    preserved: &mut Vec<PathBuf>,
    maximum: usize,
    maximum_depth: usize,
) {
    let mut stack = vec![root.to_path_buf()];
    let mut entries_seen = 0_usize;
    while let Some(directory) = stack.pop() {
        let Ok(relative) = directory.strip_prefix(root) else {
            push_preserved(preserved, root.to_path_buf(), maximum);
            return;
        };
        if relative.components().count() > maximum_depth {
            push_preserved(preserved, root.to_path_buf(), maximum);
            return;
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                push_preserved(preserved, root.to_path_buf(), maximum);
                return;
            }
        };
        let mut empty = true;
        for entry in entries {
            entries_seen += 1;
            if entries_seen > maximum {
                push_preserved(preserved, root.to_path_buf(), maximum);
                return;
            }
            empty = false;
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(_) => {
                    push_preserved(preserved, root.to_path_buf(), maximum);
                    return;
                }
            };
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    push_preserved(preserved, root.to_path_buf(), maximum);
                    return;
                }
            };
            if metadata.is_dir() && !is_reparse_point(&metadata) {
                stack.push(path);
            } else {
                push_preserved(preserved, path, maximum);
            }
        }
        if empty && directory != root {
            push_preserved(preserved, directory, maximum);
        }
    }
}

pub(super) fn remove_empty_parents(mut parent: Option<&Path>, stop: &Path) {
    while let Some(directory) = parent {
        if directory == stop || std::fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}

fn push_preserved(paths: &mut Vec<PathBuf>, path: PathBuf, maximum: usize) {
    if paths.len() < maximum && !paths.contains(&path) {
        paths.push(path);
    }
}

pub(super) fn snapshot_regular(path: &Path, maximum: u64) -> Result<Option<(u64, String)>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > maximum {
        return Ok(None);
    }
    let mut file = open_read_locked(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || is_reparse_point(&opened) || opened.len() != metadata.len() {
        bail!("external tool recovery source changed while opening");
    }
    let sha256 = hash_reader(&mut file)?;
    if file.metadata()?.len() != opened.len() {
        bail!("external tool recovery source changed while hashing");
    }
    Ok(Some((opened.len(), sha256)))
}

pub(super) fn delete_if_exact(path: &Path, size_bytes: u64, sha256: &str) -> Result<bool> {
    let mut file = match open_cleanup_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() != size_bytes {
        return Ok(false);
    }
    if !hash_reader(&mut file)?.eq_ignore_ascii_case(sha256) {
        return Ok(false);
    }
    delete_open_file(&file, path)?;
    Ok(true)
}

fn hash_reader(file: &mut std::fs::File) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn open_read_locked(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
        options
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    options.open(path)
}

fn open_cleanup_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
        };
        options
            .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    options.open(path)
}

pub(super) fn is_windows_reserved_name(value: &str) -> bool {
    let stem = value.split('.').next().unwrap_or(value);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
    ) || (stem.len() == 4
        && matches!(stem[..3].to_ascii_uppercase().as_str(), "COM" | "LPT")
        && stem.as_bytes()[3].is_ascii_digit()
        && stem.as_bytes()[3] != b'0')
}

#[cfg(windows)]
fn delete_open_file(file: &std::fs::File, _path: &Path) -> Result<()> {
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
    }?;
    Ok(())
}

#[cfg(not(windows))]
fn delete_open_file(_file: &std::fs::File, path: &Path) -> Result<()> {
    std::fs::remove_file(path)?;
    Ok(())
}
