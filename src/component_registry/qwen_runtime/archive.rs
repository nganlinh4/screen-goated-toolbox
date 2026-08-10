use std::collections::HashSet;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::super::receipt::{file_matches, is_reparse_point, resolve_owned_path};
use super::{MAX_COMPONENT_FILES, QwenRuntimeDelivery, QwenRuntimeFile, owned_file};

const MAX_LIBTORCH_ENTRIES: usize = 12_000;

pub(super) fn extract_archive(
    archive_path: &Path,
    archive_index: usize,
    staging: &Path,
    delivery: &QwenRuntimeDelivery,
) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > MAX_LIBTORCH_ENTRIES {
        bail!("Qwen3 runtime archive has too many entries");
    }
    let expected = delivery
        .files
        .iter()
        .filter(|file| file.archive_index == archive_index)
        .collect::<Vec<_>>();
    if archive.len() != expected.len() {
        bail!("Qwen3 runtime asset contains unexpected entries");
    }
    let mut extracted = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else {
            bail!("Qwen3 archive contains an unsafe path");
        };
        let Some(expected_file) = expected
            .iter()
            .find(|file| Path::new(file.archive_path) == path)
            .copied()
        else {
            bail!("Qwen3 archive contains an unexpected file");
        };
        if !extracted.insert(expected_file.archive_path) || entry.size() != expected_file.size_bytes
        {
            bail!("Qwen3 archive entry does not match its delivery manifest");
        }
        let target = prepare_target(staging, Path::new(expected_file.path))?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &owned_file(expected_file))? {
            bail!("extracted Qwen3 runtime file failed integrity verification");
        }
        if target
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("dll"))
        {
            validate_x64_pe(&target)?;
        }
    }
    if extracted.len() != expected.len() {
        bail!("Qwen3 archive is missing required files");
    }
    Ok(())
}

pub(super) fn validate_x64_pe(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to inspect native file '{}'", path.display()))?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("native file '{}' is not PE", path.display());
    }
    let pe_offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if pe_offset > 1024 * 1024 {
        bail!("native file '{}' has an invalid PE offset", path.display());
    }
    file.seek(SeekFrom::Start(pe_offset))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("native file '{}' is not Windows x64", path.display());
    }
    Ok(())
}

pub(super) fn prepare_target(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_regular_directory(root)?;
    let mut current = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let Component::Normal(name) = component else {
                bail!("Qwen3 staging parent is unsafe");
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

pub(super) fn cleanup_owned(root: &Path, files: &[QwenRuntimeFile]) -> Result<()> {
    validate_regular_directory(root)?;
    let mut directories = HashSet::new();
    for file in files.iter().take(MAX_COMPONENT_FILES) {
        let relative = Path::new(file.path);
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

pub(super) fn ensure_working_directory(root: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    validate_regular_directory(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow!("Qwen3 working directory escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("Qwen3 working directory is unsafe");
        };
        current.push(name);
        match std::fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        validate_regular_directory(&current)?;
    }
    Ok(())
}

fn validate_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("Qwen3 staging path is not a regular directory");
    }
    Ok(())
}
