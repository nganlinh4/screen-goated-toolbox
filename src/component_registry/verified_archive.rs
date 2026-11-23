//! Inventory-bound ZIP traversal. Callers retain staging/publication ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, ensure};

use super::receipt::{RECEIPT_NAME, validate_relative_path};
use super::verified_copy;
use super::windows_path::WindowsPath;

pub(super) struct FileSpec<'a> {
    archive_path: &'a Path,
    path: &'a Path,
    size: u64,
    digest: &'a str,
}

impl<'a> FileSpec<'a> {
    pub(super) fn same(path: &'a Path, size: u64, digest: &'a str) -> Self {
        Self::mapped(path, path, size, digest)
    }

    pub(super) fn mapped(
        archive_path: &'a Path,
        path: &'a Path,
        size: u64,
        digest: &'a str,
    ) -> Self {
        Self {
            archive_path,
            path,
            size,
            digest,
        }
    }
}

pub(super) struct Policy {
    pub(super) max_entries: usize,
    pub(super) size: u64,
    pub(super) directories: Directories,
}

#[derive(Clone, Copy)]
pub(super) enum Directories {
    Forbidden,
    #[cfg(any(test, not(feature = "recorder-worker")))]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
        )
    )]
    Empty,
}

impl Policy {
    pub(super) fn files(max_entries: usize, size: u64) -> Self {
        Self {
            max_entries,
            size,
            directories: Directories::Forbidden,
        }
    }
}

pub(super) struct Member<'zip, 'context, R: Read> {
    entry: zip::read::ZipFile<'zip, R>,
    size: u64,
    digest: &'context str,
    cancelled: &'context AtomicBool,
    verified: bool,
}

impl<R: Read> Member<'_, '_, R> {
    pub(super) fn copy_to(&mut self, output: &mut dyn Write, progress: &dyn Fn(u64)) -> Result<()> {
        ensure!(!self.verified, "archive member was already verified");
        verified_copy::copy_verified(
            &mut self.entry,
            output,
            self.size,
            self.digest,
            self.cancelled,
            progress,
        )?;
        self.verified = true;
        Ok(())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
        )
    )]
    pub(super) fn retain_verified(&mut self, path: &Path) -> Result<bool> {
        ensure!(!self.verified, "archive member was already verified");
        let file = match super::native_file::open_regular(path) {
            Ok(file) => file,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() != self.size {
            return Ok(false);
        }
        self.verified = verified_copy::matches(
            file,
            &mut std::io::sink(),
            self.size,
            self.digest,
            self.cancelled,
            &|_| {},
        )?;
        Ok(self.verified)
    }
}

pub(super) fn extract<R: Read + Seek>(
    reader: R,
    files: &[FileSpec<'_>],
    policy: Policy,
    cancelled: &AtomicBool,
    mut install: impl FnMut(usize, &mut Member<'_, '_, R>) -> Result<()>,
) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "archive extraction cancelled"
    );
    let expected = validate_inventory(files, &policy)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    ensure!(
        !archive.is_empty() && archive.len() <= policy.max_entries,
        "archive entry count is invalid"
    );
    if matches!(policy.directories, Directories::Forbidden) {
        ensure!(
            archive.len() == files.len(),
            "archive inventory count differs"
        );
    }
    let mut names = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(archive.len());
    // Validate the entire namespace before invoking any output callback.
    for index in 0..archive.len() {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "archive extraction cancelled"
        );
        let entry = archive.by_index(index)?;
        ensure!(
            !entry.encrypted() && !entry.is_symlink() && entry.external_attributes() & 0x400 == 0,
            "archive contains an encrypted or non-regular entry"
        );
        let name = std::str::from_utf8(entry.name_raw())?;
        let directory = entry.is_dir();
        let relative = if directory {
            name.strip_suffix('/').unwrap_or(name)
        } else {
            name
        };
        canonical_archive_path(Path::new(relative))?;
        let key = WindowsPath::new(Path::new(relative));
        ensure!(
            names
                .insert(key, (directory, Path::new(relative).to_path_buf()))
                .is_none(),
            "archive has aliased entries"
        );
        let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
        ensure!(
            mode == 0 || mode == if directory { 0o040000 } else { 0o100000 },
            "archive contains a non-regular entry"
        );
        if directory {
            ensure!(
                !matches!(policy.directories, Directories::Forbidden) && entry.size() == 0,
                "archive contains an unexpected directory entry"
            );
            ensure!(
                !expected.contains_key(&WindowsPath::new(Path::new(relative))),
                "archive uses a declared file as a directory"
            );
            selected.push(None);
            continue;
        }
        let file_index = *expected
            .get(&WindowsPath::new(Path::new(relative)))
            .ok_or_else(|| anyhow::anyhow!("archive contains an undeclared file"))?;
        let file = &files[file_index];
        ensure!(
            file.archive_path.to_str() == Some(relative)
                && entry.size() == file.size
                && seen.insert(file_index),
            "archive member differs from its pinned inventory"
        );
        selected.push(Some(file_index));
    }
    ensure!(
        seen.len() == files.len(),
        "archive is missing declared files"
    );
    for (_, path) in names.values() {
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            ensure!(
                !names
                    .get(&WindowsPath::new(parent))
                    .is_some_and(|(directory, _)| !directory),
                "archive file is also a directory parent"
            );
        }
    }
    for (index, file_index) in selected.into_iter().enumerate() {
        let Some(file_index) = file_index else {
            continue;
        };
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "archive extraction cancelled"
        );
        let file = &files[file_index];
        let mut member = Member {
            entry: archive.by_index(index)?,
            size: file.size,
            digest: file.digest,
            cancelled,
            verified: false,
        };
        install(file_index, &mut member)?;
        ensure!(
            member.verified,
            "archive member was not verified before completion"
        );
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "archive extraction cancelled"
        );
    }
    Ok(())
}

fn canonical_archive_path(path: &Path) -> Result<()> {
    validate_relative_path(path)?;
    ensure!(
        path.to_str().is_some_and(|path| !path.contains('\\')),
        "archive path is not canonical"
    );
    Ok(())
}

fn validate_inventory(
    files: &[FileSpec<'_>],
    policy: &Policy,
) -> Result<BTreeMap<WindowsPath, usize>> {
    ensure!(
        !files.is_empty() && files.len() <= policy.max_entries,
        "archive inventory count is invalid"
    );
    let mut sources = BTreeMap::new();
    let mut targets = BTreeSet::new();
    let mut total = 0_u64;
    for (index, file) in files.iter().enumerate() {
        canonical_archive_path(file.archive_path)?;
        validate_relative_path(file.path)?;
        ensure!(
            file.digest.len() == 64 && file.digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "archive file digest is invalid"
        );
        ensure!(
            sources
                .insert(WindowsPath::new(file.archive_path), index)
                .is_none()
                && targets.insert(WindowsPath::new(file.path)),
            "archive inventory has aliased paths"
        );
        ensure!(
            WindowsPath::new(file.path) != WindowsPath::new(Path::new(RECEIPT_NAME)),
            "archive targets reserved receipt metadata"
        );
        total = total
            .checked_add(file.size)
            .ok_or_else(|| anyhow::anyhow!("archive size overflow"))?;
    }
    ensure!(
        total == policy.size,
        "archive expansion differs from its delivery contract"
    );
    for file in files {
        for parent in file
            .archive_path
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            ensure!(
                !sources.contains_key(&WindowsPath::new(parent)),
                "archive file is also a parent"
            );
        }
        for parent in file
            .path
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            ensure!(
                !targets.contains(&WindowsPath::new(parent))
                    && WindowsPath::new(parent) != WindowsPath::new(Path::new(RECEIPT_NAME)),
                "archive target file is also a parent"
            );
        }
    }
    Ok(sources)
}

#[cfg(test)]
#[path = "verified_archive_tests.rs"]
mod tests;
