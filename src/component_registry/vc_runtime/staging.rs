use std::collections::HashSet;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::component_registry::receipt::{
    is_reparse_point, resolve_owned_path, validate_relative_path,
};

pub(super) fn prepare_target(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    validate_regular_directory(root)?;
    let mut current = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let Component::Normal(name) = component else {
                bail!("VC runtime parent path is unsafe");
            };
            current.push(name);
            match std::fs::create_dir(&current) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
            validate_regular_directory(&current)?;
        }
    }
    resolve_owned_path(root, relative)
}

pub(super) fn cleanup_owned(root: &Path, relative_paths: &[&str]) -> Result<()> {
    validate_regular_directory(root)?;
    let mut directories = HashSet::new();
    for relative in relative_paths {
        let relative = Path::new(relative);
        let path = match resolve_owned_path(root, relative) {
            Ok(path) => path,
            Err(_) => continue,
        };
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.is_file()
            && !is_reparse_point(&metadata)
        {
            std::fs::remove_file(&path)?;
        }
        let mut parent = relative.parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for relative in directories {
        let path = root.join(relative);
        if validate_regular_directory(&path).is_ok() {
            match std::fs::remove_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    validate_regular_directory(root)?;
    match std::fs::remove_dir(root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn validate_x64_pe(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("VC runtime file is not PE");
    }
    let pe_offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if pe_offset > 1024 * 1024 {
        bail!("VC runtime file has an invalid PE offset");
    }
    file.seek(SeekFrom::Start(pe_offset))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("VC runtime file is not Windows x64");
    }
    Ok(())
}

fn validate_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("VC runtime staging path is not a regular directory");
    }
    Ok(())
}
