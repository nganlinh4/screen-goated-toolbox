//! Windows transaction boundary for exact text edits.

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_DISPOSITION_INFO, FILE_GENERIC_READ,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo,
    GetFileInformationByHandle, REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
    SetFileInformationByHandle,
};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::core::PCWSTR;

const MUTEX_WAIT_MS: u32 = 30_000;

pub(super) struct EditGuard {
    _mutex: NamedMutexGuard,
    source: Option<File>,
    identity: FileIdentity,
}

pub(super) struct CurrentPathGuard {
    _file: File,
}

pub(super) struct StagedFile {
    path: PathBuf,
    file: Option<File>,
    identity: FileIdentity,
    cleanup_on_drop: bool,
}

pub(super) enum CommitOutcome {
    Verified {
        retained_backup: Option<PathBuf>,
    },
    NoEffect {
        error: String,
    },
    Ambiguous {
        error: String,
        tool_mutated_file: bool,
        external_change_detected: bool,
        recovery_backup: Option<PathBuf>,
        recovery_sha256: Option<String>,
    },
}

#[derive(Debug)]
pub(super) struct ConcurrentChange {
    pub(super) actual_hash: Option<String>,
}

#[derive(Debug)]
pub(super) enum TransactionFailure {
    Busy(String),
    Missing,
    NotFile,
    Oversize(u64),
    Io(String),
}

impl EditGuard {
    pub(super) fn acquire(path: &Path) -> Result<Self, TransactionFailure> {
        let mutex = NamedMutexGuard::acquire(path)?;
        let source = open_transaction_handle(path)?;
        let identity = file_identity(&source)?;
        Ok(Self {
            _mutex: mutex,
            source: Some(source),
            identity,
        })
    }

    pub(super) fn read_bounded(&mut self, max_bytes: u64) -> Result<Vec<u8>, TransactionFailure> {
        read_file_bounded(
            self.source
                .as_mut()
                .expect("source handle exists until commit"),
            max_bytes,
        )
    }

    pub(super) fn validate_current(
        &self,
        path: &Path,
        original: &[u8],
        max_bytes: u64,
    ) -> Result<CurrentPathGuard, ConcurrentChange> {
        let mut current = open_transaction_handle(path).map_err(|_| ConcurrentChange {
            actual_hash: std::fs::read(path).ok().map(|bytes| sha256_hex(&bytes)),
        })?;
        let identity =
            file_identity(&current).map_err(|_| ConcurrentChange { actual_hash: None })?;
        if identity != self.identity {
            return Err(ConcurrentChange {
                actual_hash: snapshot(path, max_bytes)
                    .ok()
                    .map(|snapshot| sha256_hex(&snapshot.bytes)),
            });
        }
        let current_bytes = read_file_bounded(&mut current, max_bytes)
            .map_err(|_| ConcurrentChange { actual_hash: None })?;
        if current_bytes != original {
            return Err(ConcurrentChange {
                actual_hash: Some(sha256_hex(&current_bytes)),
            });
        }
        Ok(CurrentPathGuard { _file: current })
    }

    pub(super) fn commit_audited(
        mut self,
        path: &Path,
        current: CurrentPathGuard,
        staged: StagedFile,
        original: &[u8],
        edited: &[u8],
        max_bytes: u64,
    ) -> CommitOutcome {
        // ReplaceFileW rejects live handles to its operands on supported Windows
        // versions even when all share flags are present. Close only after both
        // identities and byte sequences have been captured; the atomic backup
        // below is the commit-time audit record for intervening writers.
        drop(current);
        drop(self.source.take());
        let (staged_path, staged_identity) = staged.close_for_replace();
        let backup = unique_sibling(path, "backup");
        let replacement = replace_file(path, &staged_path, Some(&backup));
        if let Err(error) = replacement {
            let target_snapshot = snapshot(path, max_bytes).ok();
            let backup_snapshot = snapshot(&backup, max_bytes).ok();
            let unchanged = target_snapshot.as_ref().is_some_and(|current| {
                current.identity == self.identity && current.bytes == original
            });
            let tool_mutated_file = target_snapshot
                .as_ref()
                .is_some_and(|current| current.identity == staged_identity);
            let backup_differs_from_original = backup_snapshot
                .as_ref()
                .is_some_and(|old| old.identity != self.identity || old.bytes != original);
            let external_change_detected = backup_differs_from_original
                || target_snapshot.as_ref().is_none_or(|current| {
                    if current.identity == staged_identity {
                        current.bytes != edited
                    } else {
                        current.identity != self.identity || current.bytes != original
                    }
                });
            let _ = remove_if_identity(&staged_path, staged_identity);
            return if unchanged {
                let _ = remove_if_identity(&backup, self.identity);
                CommitOutcome::NoEffect {
                    error: format!("atomic replacement failed before changing the target: {error}"),
                }
            } else {
                CommitOutcome::Ambiguous {
                    error: format!(
                        "atomic replacement returned an error and target identity is no longer provable: {error}"
                    ),
                    tool_mutated_file,
                    external_change_detected,
                    recovery_backup: existing_path(&backup),
                    recovery_sha256: backup_snapshot
                        .as_ref()
                        .map(|snapshot| sha256_hex(&snapshot.bytes)),
                }
            };
        }

        // ReplaceFileW moved the exact file occupying `path` to `backup` in the
        // same atomic namespace operation that installed the staged file. The
        // backup identity is therefore the old-file audit record.
        let backup_snapshot = snapshot(&backup, max_bytes).ok();
        let target_snapshot = snapshot(path, max_bytes).ok();
        let old_was_validated = backup_snapshot
            .as_ref()
            .is_some_and(|old| old.identity == self.identity && old.bytes == original);
        let tool_mutated_file = target_snapshot
            .as_ref()
            .is_some_and(|current| current.identity == staged_identity);
        let new_is_staged = target_snapshot
            .as_ref()
            .is_some_and(|current| current.identity == staged_identity && current.bytes == edited);
        if old_was_validated && new_is_staged {
            let retained_backup = remove_if_identity(&backup, self.identity)
                .err()
                .and_then(|_| existing_path(&backup));
            return CommitOutcome::Verified { retained_backup };
        }

        let recovery_sha256 = backup_snapshot
            .as_ref()
            .map(|snapshot| sha256_hex(&snapshot.bytes));
        CommitOutcome::Ambiguous {
            error: if !old_was_validated {
                "the pathname was replaced after validation; competing bytes were preserved"
                    .to_string()
            } else {
                "the staged file was installed but no longer owns the pathname".to_string()
            },
            tool_mutated_file,
            external_change_detected: true,
            recovery_backup: existing_path(&backup),
            recovery_sha256,
        }
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        drop(self.file.take());
        if self.cleanup_on_drop {
            let _ = remove_if_identity(&self.path, self.identity);
        }
    }
}

impl StagedFile {
    fn close_for_replace(mut self) -> (PathBuf, FileIdentity) {
        drop(self.file.take());
        self.cleanup_on_drop = false;
        (self.path.clone(), self.identity)
    }

    #[cfg(test)]
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileIdentity {
    volume: u32,
    index: u64,
}

struct Snapshot {
    identity: FileIdentity,
    bytes: Vec<u8>,
}

struct NamedMutexGuard {
    handle: HANDLE,
}

impl NamedMutexGuard {
    fn acquire(path: &Path) -> Result<Self, TransactionFailure> {
        let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
        let digest = Sha256::digest(normalized.as_bytes());
        let suffix: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        let name: Vec<u16> = format!("Local\\SGT.CC.TextEdit.{suffix}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|error| TransactionFailure::Io(error.to_string()))?;
        let wait = unsafe { WaitForSingleObject(handle, MUTEX_WAIT_MS) };
        if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
            Ok(Self { handle })
        } else {
            unsafe {
                let _ = CloseHandle(handle);
            }
            if wait == WAIT_TIMEOUT {
                Err(TransactionFailure::Busy(
                    "another exact edit still owns this path".to_string(),
                ))
            } else {
                Err(TransactionFailure::Io(format!(
                    "waiting for the path edit mutex failed with status {}",
                    wait.0
                )))
            }
        }
    }
}

impl Drop for NamedMutexGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

fn open_transaction_handle(path: &Path) -> Result<File, TransactionFailure> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .open(path)
        .map_err(classify_open_error)
}

fn classify_open_error(error: std::io::Error) -> TransactionFailure {
    match error.raw_os_error() {
        Some(32 | 33) => TransactionFailure::Busy(
            "another writer has the file open; no bytes were changed".to_string(),
        ),
        _ if error.kind() == std::io::ErrorKind::NotFound => TransactionFailure::Missing,
        _ => TransactionFailure::Io(error.to_string()),
    }
}

fn read_file_bounded(file: &mut File, max_bytes: u64) -> Result<Vec<u8>, TransactionFailure> {
    let metadata = file
        .metadata()
        .map_err(|error| TransactionFailure::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(TransactionFailure::NotFile);
    }
    if metadata.len() > max_bytes {
        return Err(TransactionFailure::Oversize(metadata.len()));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| TransactionFailure::Io(error.to_string()))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| TransactionFailure::Io(error.to_string()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(TransactionFailure::Oversize(bytes.len() as u64));
    }
    Ok(bytes)
}

pub(super) fn file_identity(file: &File) -> Result<FileIdentity, TransactionFailure> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)
            .map_err(|error| TransactionFailure::Io(error.to_string()))?;
    }
    Ok(FileIdentity {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

pub(super) fn write_synced_sibling(path: &Path, bytes: &[u8]) -> std::io::Result<StagedFile> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("text");
    for _ in 0..32 {
        let nonce = NEXT.fetch_add(1, Ordering::SeqCst);
        let temporary = parent.join(format!(
            ".{name}.sgt-edit-{}-{nonce}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
            .open(&temporary)
        {
            Ok(mut file) => {
                if let Err(error) = write_and_sync(&mut file, bytes) {
                    drop(file);
                    let _ = std::fs::remove_file(&temporary);
                    return Err(error);
                }
                let identity = file_identity(&file).map_err(transaction_io_error)?;
                drop(file);
                let mut file = open_transaction_handle(&temporary).map_err(transaction_io_error)?;
                let reopened_identity = file_identity(&file).map_err(transaction_io_error)?;
                let readback = read_file_bounded(&mut file, bytes.len() as u64)
                    .map_err(transaction_io_error)?;
                if reopened_identity != identity || readback != bytes {
                    let _ = remove_if_identity(&temporary, identity);
                    return Err(std::io::Error::other(
                        "staged file identity or bytes changed before commit",
                    ));
                }
                return Ok(StagedFile {
                    path: temporary,
                    file: Some(file),
                    identity,
                    cleanup_on_drop: true,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique sibling temporary file",
    ))
}

fn write_and_sync(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()
}

#[cfg(test)]
pub(super) fn atomic_replace(path: &Path, replacement: StagedFile) -> windows::core::Result<()> {
    let (replacement_path, replacement_identity) = replacement.close_for_replace();
    let result = replace_file(path, &replacement_path, None);
    if result.is_err() {
        let _ = remove_if_identity(&replacement_path, replacement_identity);
    }
    result
}

fn replace_file(
    path: &Path,
    replacement: &Path,
    backup: Option<&Path>,
) -> windows::core::Result<()> {
    let target_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement_wide: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let backup_wide = backup.map(|path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    });
    let backup = backup_wide
        .as_ref()
        .map_or(PCWSTR::null(), |path| PCWSTR(path.as_ptr()));
    unsafe {
        ReplaceFileW(
            PCWSTR(target_wide.as_ptr()),
            PCWSTR(replacement_wide.as_ptr()),
            backup,
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
}

fn snapshot(path: &Path, max_bytes: u64) -> Result<Snapshot, TransactionFailure> {
    let mut file = open_transaction_handle(path)?;
    let identity = file_identity(&file)?;
    let bytes = read_file_bounded(&mut file, max_bytes)?;
    Ok(Snapshot { identity, bytes })
}

fn remove_if_identity(path: &Path, expected: FileIdentity) -> std::io::Result<()> {
    let file = OpenOptions::new()
        .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .open(path)?;
    let actual = file_identity(&file).map_err(transaction_io_error)?;
    if actual != expected {
        return Err(std::io::Error::other(
            "file identity changed before cleanup",
        ));
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
        .map_err(std::io::Error::other)?;
    }
    drop(file);
    Ok(())
}

fn existing_path(path: &Path) -> Option<PathBuf> {
    path.exists().then(|| path.to_path_buf())
}

fn unique_sibling(path: &Path, role: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(10_000);
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("text");
    let nonce = NEXT.fetch_add(1, Ordering::SeqCst);
    parent.join(format!(
        ".{name}.sgt-{role}-{}-{nonce}.tmp",
        std::process::id()
    ))
}

fn transaction_io_error(error: TransactionFailure) -> std::io::Error {
    let message = match error {
        TransactionFailure::Busy(message) | TransactionFailure::Io(message) => message,
        TransactionFailure::Missing => "file is missing".to_string(),
        TransactionFailure::NotFile => "path is not a file".to_string(),
        TransactionFailure::Oversize(size) => format!("file is too large: {size} bytes"),
    };
    std::io::Error::other(message)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
