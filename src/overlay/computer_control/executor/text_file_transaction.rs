//! Windows transaction boundary for exact text edits.

use crate::crypto::sha256_hex;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
};
use windows::core::PCWSTR;

#[path = "text_file_cleanup.rs"]
mod cleanup;
use cleanup::{remove_backup, remove_if_identity};
#[path = "text_file_mutex.rs"]
mod path_mutex;
use path_mutex::NamedMutexGuard;

pub(in crate::overlay::computer_control) struct CommitOptions<'a> {
    pub(in crate::overlay::computer_control) max_bytes: u64,
    pub(in crate::overlay::computer_control) cancel: &'a std::sync::atomic::AtomicBool,
}

pub(in crate::overlay::computer_control) struct EditGuard {
    _mutex: NamedMutexGuard,
    source: Option<File>,
    identity: FileIdentity,
}

pub(in crate::overlay::computer_control) struct CurrentPathGuard {
    _file: File,
}

pub(in crate::overlay::computer_control) struct StagedFile {
    path: PathBuf,
    file: Option<File>,
    identity: FileIdentity,
    cleanup_on_drop: bool,
}

pub(in crate::overlay::computer_control) enum CommitOutcome {
    Verified {
        retained_backup: Option<PathBuf>,
    },
    NoEffect {
        error: String,
        retained_staging: Option<PathBuf>,
    },
    Ambiguous {
        error: String,
        tool_mutated_file: bool,
        external_change_detected: bool,
        recovery_backup: Option<PathBuf>,
        recovery_sha256: Option<String>,
        retained_staging: Option<PathBuf>,
    },
}

#[derive(Debug)]
pub(in crate::overlay::computer_control) struct ConcurrentChange {
    pub(in crate::overlay::computer_control) actual_hash: Option<String>,
}

#[derive(Debug)]
pub(in crate::overlay::computer_control) enum TransactionFailure {
    Busy(String),
    Missing,
    NotFile,
    Oversize(u64),
    Io(String),
}

impl EditGuard {
    pub(in crate::overlay::computer_control) fn acquire(
        path: &Path,
    ) -> Result<Self, TransactionFailure> {
        let mutex = NamedMutexGuard::acquire(path)?;
        let source = open_transaction_handle(path)?;
        let identity = file_identity(&source)?;
        Ok(Self {
            _mutex: mutex,
            source: Some(source),
            identity,
        })
    }

    pub(in crate::overlay::computer_control) fn identity(&self) -> FileIdentity {
        self.identity
    }

    pub(in crate::overlay::computer_control) fn read_bounded(
        &mut self,
        max_bytes: u64,
    ) -> Result<Vec<u8>, TransactionFailure> {
        read_file_bounded(
            self.source
                .as_mut()
                .expect("source handle exists until commit"),
            max_bytes,
        )
    }

    pub(in crate::overlay::computer_control) fn validate_current(
        &self,
        path: &Path,
        original: &[u8],
        max_bytes: u64,
    ) -> Result<CurrentPathGuard, ConcurrentChange> {
        let mut current =
            open_transaction_handle(path).map_err(|_| ConcurrentChange { actual_hash: None })?;
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

    pub(in crate::overlay::computer_control) fn commit_audited(
        self,
        path: &Path,
        current: CurrentPathGuard,
        staged: StagedFile,
        original: &[u8],
        edited: &[u8],
        max_bytes: u64,
    ) -> CommitOutcome {
        self.commit_inner(path, current, staged, original, edited, (max_bytes, None))
    }

    pub(in crate::overlay::computer_control) fn commit_cancellable(
        self,
        path: &Path,
        current: CurrentPathGuard,
        staged: StagedFile,
        original: &[u8],
        edited: &[u8],
        options: CommitOptions<'_>,
    ) -> CommitOutcome {
        self.commit_inner(
            path,
            current,
            staged,
            original,
            edited,
            (options.max_bytes, Some(options.cancel)),
        )
    }

    fn commit_inner(
        mut self,
        path: &Path,
        current: CurrentPathGuard,
        staged: StagedFile,
        original: &[u8],
        edited: &[u8],
        settings: (u64, Option<&std::sync::atomic::AtomicBool>),
    ) -> CommitOutcome {
        let (max_bytes, cancel) = settings;
        // ReplaceFileW rejects live handles to its operands on supported Windows
        // versions even when all share flags are present. Close only after both
        // identities and byte sequences have been captured; the atomic backup
        // below is the commit-time audit record for intervening writers.
        drop(current);
        drop(self.source.take());
        let backup = match unique_sibling(path, "backup") {
            Ok(path) => path,
            Err(error) => {
                return no_effect(error.to_string(), staged);
            }
        };
        let (staged_path, staged_identity) = staged.close_for_replace();
        let replacement =
            crate::overlay::computer_control::executor::with_job_admission(cancel, || {
                replace_file(path, &staged_path, Some(&backup))
            })
            .unwrap_or_else(|| {
                Err(windows::core::Error::from_hresult(
                    windows::core::HRESULT::from_win32(1223),
                ))
            });
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
            let retained_staging = remove_if_identity(&staged_path, staged_identity)
                .err()
                .map(|_| staged_path.clone());
            let backup_retained = unchanged
                && backup.try_exists().unwrap_or(true)
                && remove_backup(&backup, self.identity, original, max_bytes).is_err();
            return if unchanged && !backup_retained {
                CommitOutcome::NoEffect {
                    error: format!("atomic replacement failed before changing the target: {error}"),
                    retained_staging,
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
                    retained_staging,
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
            let retained_backup = remove_backup(&backup, self.identity, original, max_bytes)
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
            retained_staging: None,
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
    pub(in crate::overlay::computer_control) fn publish_new(
        mut self,
        parent: &File,
        path: &Path,
        expected: &[u8],
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> CommitOutcome {
        let file = self.file.as_ref().expect("owned staged file");
        let publication =
            crate::overlay::computer_control::executor::with_job_admission(cancel, || {
                crate::overlay::computer_control::native_file::publish_new(
                    file,
                    parent,
                    path,
                    windows::Win32::Storage::FileSystem::VOLUME_NAME_DOS,
                )
            })
            .unwrap_or_else(|| Err(anyhow::anyhow!("artifact publication cancelled")));
        if let Err(error) = publication {
            return no_effect(error.to_string(), self);
        }
        self.cleanup_on_drop = false;
        // Retain the publishing handle: its sharing denies writes until
        // the exact installed object and its bytes have been checked.
        match snapshot(path, expected.len() as u64) {
            Ok(current) if current.identity == self.identity && current.bytes == expected => {
                CommitOutcome::Verified {
                    retained_backup: None,
                }
            }
            _ => CommitOutcome::Ambiguous {
                error: "published file could not be verified".into(),
                tool_mutated_file: true,
                external_change_detected: true,
                recovery_backup: None,
                recovery_sha256: None,
                retained_staging: None,
            },
        }
    }

    pub(in crate::overlay::computer_control) fn discard(mut self) -> std::io::Result<()> {
        drop(self.file.take());
        let result = remove_if_identity(&self.path, self.identity);
        self.cleanup_on_drop = false;
        result
    }

    fn close_for_replace(mut self) -> (PathBuf, FileIdentity) {
        drop(self.file.take());
        self.cleanup_on_drop = false;
        (self.path.clone(), self.identity)
    }

    pub(in crate::overlay::computer_control) fn path(&self) -> &Path {
        &self.path
    }
}

pub(in crate::overlay::computer_control) use crate::overlay::computer_control::native_file::Identity as FileIdentity;

struct Snapshot {
    identity: FileIdentity,
    bytes: Vec<u8>,
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

pub(in crate::overlay::computer_control) fn file_identity(
    file: &File,
) -> Result<FileIdentity, TransactionFailure> {
    FileIdentity::read(file).map_err(|error| TransactionFailure::Io(error.to_string()))
}

pub(in crate::overlay::computer_control) fn write_synced_sibling(
    path: &Path,
    bytes: &[u8],
) -> std::io::Result<StagedFile> {
    write_sibling_with(path, bytes, write_and_sync)
}

pub(in crate::overlay::computer_control) fn write_sibling_with(
    path: &Path,
    bytes: &[u8],
    mut write: impl FnMut(&mut File, &[u8]) -> std::io::Result<()>,
) -> std::io::Result<StagedFile> {
    for _ in 0..32 {
        let temporary = unique_sibling(path, "edit")?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
            .open(&temporary)
        {
            Ok(mut file) => {
                let identity = file_identity(&file).map_err(transaction_io_error)?;
                let mut staged = StagedFile {
                    path: temporary.clone(),
                    file: None,
                    identity,
                    cleanup_on_drop: true,
                };
                if let Err(error) = write(&mut file, bytes) {
                    drop(file);
                    return Err(error);
                }
                drop(file);
                let mut file = OpenOptions::new()
                    .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
                    .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
                    .open(&temporary)?;
                let reopened_identity = file_identity(&file).map_err(transaction_io_error)?;
                let readback = read_file_bounded(&mut file, bytes.len() as u64)
                    .map_err(transaction_io_error)?;
                if reopened_identity != identity || readback != bytes {
                    let _ = remove_if_identity(&temporary, identity);
                    return Err(std::io::Error::other(
                        "staged file identity or bytes changed before commit",
                    ));
                }
                staged.file = Some(file);
                return Ok(staged);
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
pub(in crate::overlay::computer_control) fn atomic_replace(
    path: &Path,
    replacement: StagedFile,
) -> windows::core::Result<()> {
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

fn existing_path(path: &Path) -> Option<PathBuf> {
    path.try_exists()
        .unwrap_or(true)
        .then(|| path.to_path_buf())
}

fn no_effect(error: String, staged: StagedFile) -> CommitOutcome {
    let path = staged.path().to_owned();
    let retained_staging = staged.discard().err().map(|_| path);
    CommitOutcome::NoEffect {
        error,
        retained_staging,
    }
}

fn unique_sibling(path: &Path, role: &str) -> std::io::Result<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(std::io::Error::other)?;
    Ok(parent.join(format!(
        ".sgt-{role}-{:032x}.tmp",
        u128::from_le_bytes(nonce)
    )))
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
