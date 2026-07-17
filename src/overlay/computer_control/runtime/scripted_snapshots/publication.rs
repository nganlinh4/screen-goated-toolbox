//! No-clobber publication for a fully authenticated staged turn.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo,
    GetFileInformationByHandleEx, MoveFileW,
};
use windows::core::PCWSTR;

use super::stability::{ArtifactProof, PublishedArtifact, StabilityError, StagedArtifact};

const STAGING_ATTEMPTS: usize = 16;

#[derive(Debug)]
pub(super) struct PublicationError {
    pub(super) reason: &'static str,
    detail: String,
}

impl PublicationError {
    fn new(reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.reason, self.detail)
    }
}

impl std::error::Error for PublicationError {}

pub(super) struct StagingTurn {
    path: PathBuf,
}

impl StagingTurn {
    pub(super) fn new(root: &Path) -> Result<Self, PublicationError> {
        for _ in 0..STAGING_ATTEMPTS {
            let mut nonce = [0u8; 16];
            getrandom::fill(&mut nonce).map_err(|error| {
                PublicationError::new("staging_random_failed", error.to_string())
            })?;
            let path = root.join(format!(".snapshot-stage-{}", hex(&nonce)));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(PublicationError::new(
                        "staging_create_failed",
                        error.to_string(),
                    ));
                }
            }
        }
        Err(PublicationError::new(
            "staging_name_exhausted",
            "could not reserve a unique cryptographic staging name",
        ))
    }

    #[cfg(test)]
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn artifact_path(&self, file_index: usize) -> PathBuf {
        self.path.join(format!("file-{file_index:04}.snapshot"))
    }

    pub(super) fn authenticate_held(
        &self,
        artifacts: &mut [StagedArtifact],
    ) -> Result<(), PublicationError> {
        let expected = artifacts
            .iter()
            .map(|artifact| self.validated_name(artifact.path()))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if expected.len() != artifacts.len() {
            return Err(PublicationError::new(
                "staged_inventory_duplicate",
                "staged artifact names are not unique",
            ));
        }

        let actual = inventory(&self.path)?;
        if actual != expected {
            return Err(PublicationError::new(
                "staged_inventory_mismatch",
                "staging inventory does not exactly match the expected artifact set",
            ));
        }

        for artifact in artifacts {
            artifact.authenticate().map_err(PublicationError::from)?;
        }
        Ok(())
    }

    pub(super) fn publish(
        self,
        mut artifacts: Vec<StagedArtifact>,
        destination: &Path,
    ) -> Result<PublishedGuard, PublicationError> {
        let staging_lock = RootLock::acquire_sealed(&self.path)?;
        self.authenticate_held(&mut artifacts)?;
        let proofs = artifacts
            .iter()
            .map(|artifact| {
                Ok((
                    self.validated_name(artifact.path())?,
                    artifact.proof().map_err(PublicationError::from)?,
                ))
            })
            .collect::<Result<Vec<_>, PublicationError>>()?;
        // Windows cannot rename a directory containing open child handles. Keep
        // them restrictive through the last authentication, then close and use
        // one immediate no-replace rename. The cryptographic path bounds this gap.
        drop(artifacts);
        drop(staging_lock);
        move_no_replace(&self.path, destination)?;
        authenticate_published(destination, &proofs)
    }

    fn validated_name(&self, artifact: &Path) -> Result<OsString, PublicationError> {
        if artifact.parent() != Some(self.path.as_path()) {
            return Err(PublicationError::new(
                "staged_inventory_invalid",
                "artifact is outside the reserved staging directory",
            ));
        }
        artifact
            .file_name()
            .map(OsStr::to_os_string)
            .ok_or_else(|| {
                PublicationError::new("staged_inventory_invalid", "artifact has no file name")
            })
    }
}

pub(super) struct RootLock {
    path: PathBuf,
    file: File,
    identity: ObjectIdentity,
    share_mode: u32,
}

pub(super) struct PublishedGuard {
    // Windows directory sharing does not exclude insertion of a new child.
    // These retained handles still prevent mutation/replacement of every
    // authenticated expected artifact until success evidence is emitted.
    _directory_lock: RootLock,
    _artifacts: Vec<PublishedArtifact>,
}

impl RootLock {
    pub(super) fn acquire(path: &Path) -> Result<Self, PublicationError> {
        Self::acquire_with_share(path, (FILE_SHARE_READ | FILE_SHARE_WRITE).0)
    }

    fn acquire_sealed(path: &Path) -> Result<Self, PublicationError> {
        Self::acquire_with_share(path, FILE_SHARE_READ.0)
    }

    fn acquire_with_share(path: &Path, share_mode: u32) -> Result<Self, PublicationError> {
        let file = open_directory_lock(path, share_mode)?;
        let identity = object_identity(&file)?;
        let lock = Self {
            path: path.to_path_buf(),
            file,
            identity,
            share_mode,
        };
        lock.authenticate()?;
        Ok(lock)
    }

    pub(super) fn authenticate(&self) -> Result<(), PublicationError> {
        if object_identity(&self.file)? != self.identity {
            return Err(PublicationError::new(
                "root_handle_identity_changed",
                "retained snapshot root handle changed identity",
            ));
        }
        let path = open_directory_lock(&self.path, self.share_mode)?;
        if object_identity(&path)? != self.identity {
            return Err(PublicationError::new(
                "root_path_identity_changed",
                "snapshot root path no longer names the retained directory handle",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObjectIdentity {
    volume: u64,
    id: [u8; 16],
}

impl From<StabilityError> for PublicationError {
    fn from(error: StabilityError) -> Self {
        Self::new(error.reason, error.to_string())
    }
}

pub(super) fn move_no_replace(source: &Path, destination: &Path) -> Result<(), PublicationError> {
    if source.parent() != destination.parent() {
        return Err(PublicationError::new(
            "publication_parent_mismatch",
            "staging and destination must have the same parent",
        ));
    }
    let source_wide = wide_null(source);
    let destination_wide = wide_null(destination);
    // SAFETY: both buffers are live, NUL-terminated UTF-16 paths. MoveFileW is
    // the no-replace primitive: it fails when the destination already exists.
    unsafe {
        MoveFileW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(destination_wide.as_ptr()),
        )
    }
    .map_err(|error| {
        let reason = if destination.exists() {
            "publication_destination_exists"
        } else {
            "atomic_publish_failed"
        };
        PublicationError::new(reason, error.to_string())
    })
}

pub(super) fn authenticate_published(
    directory: &Path,
    proofs: &[(OsString, ArtifactProof)],
) -> Result<PublishedGuard, PublicationError> {
    let directory_lock = RootLock::acquire_sealed(directory)?;
    let expected = proofs
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    if inventory(directory)? != expected {
        return Err(PublicationError::new(
            "published_inventory_mismatch",
            "published inventory differs from the authenticated staging set",
        ));
    }
    let mut published = proofs
        .iter()
        .map(|(name, proof)| {
            PublishedArtifact::acquire(directory.join(name), *proof).map_err(PublicationError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for artifact in &mut published {
        artifact.authenticate().map_err(PublicationError::from)?;
    }
    if inventory(directory)? != expected {
        return Err(PublicationError::new(
            "published_inventory_mismatch",
            "published inventory changed during post-publication authentication",
        ));
    }
    directory_lock.authenticate()?;
    Ok(PublishedGuard {
        _directory_lock: directory_lock,
        _artifacts: published,
    })
}

fn inventory(directory: &Path) -> Result<BTreeSet<OsString>, PublicationError> {
    let mut actual = BTreeSet::new();
    let entries = std::fs::read_dir(directory).map_err(|error| {
        PublicationError::new("snapshot_inventory_read_failed", error.to_string())
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            PublicationError::new("snapshot_inventory_read_failed", error.to_string())
        })?;
        let file_type = entry.file_type().map_err(|error| {
            PublicationError::new("snapshot_inventory_read_failed", error.to_string())
        })?;
        if !file_type.is_file() {
            return Err(PublicationError::new(
                "snapshot_inventory_invalid",
                "snapshot inventory contains a non-file object",
            ));
        }
        actual.insert(entry.file_name());
    }
    Ok(actual)
}

fn open_directory_lock(path: &Path, share_mode: u32) -> Result<File, PublicationError> {
    OpenOptions::new()
        .read(true)
        .share_mode(share_mode)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
        .map_err(|error| PublicationError::new("snapshot_root_lock_failed", error.to_string()))
}

fn object_identity(file: &File) -> Result<ObjectIdentity, PublicationError> {
    let mut information = FILE_ID_INFO::default();
    // SAFETY: the File owns a valid handle and the correctly-sized output lives through the call.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
        .map_err(|error| {
            PublicationError::new("snapshot_root_identity_unavailable", error.to_string())
        })?;
    }
    validate_object_identity(
        information.VolumeSerialNumber,
        information.FileId.Identifier,
    )
}

fn validate_object_identity(volume: u64, id: [u8; 16]) -> Result<ObjectIdentity, PublicationError> {
    if id.iter().all(|byte| *byte == 0) || id.iter().all(|byte| *byte == u8::MAX) {
        return Err(PublicationError::new(
            "snapshot_root_identity_unsupported",
            "filesystem returned an unavailable 128-bit directory identity sentinel",
        ));
    }
    Ok(ObjectIdentity { volume, id })
}

fn wide_null(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::validate_object_identity;

    #[test]
    fn unavailable_extended_directory_identity_sentinels_fail_closed() {
        for id in [[0; 16], [u8::MAX; 16]] {
            let error = validate_object_identity(42, id).unwrap_err();
            assert_eq!(error.reason, "snapshot_root_identity_unsupported");
        }
    }

    #[test]
    fn nonzero_extended_directory_identity_is_accepted() {
        let mut id = [0; 16];
        id[0] = 1;
        assert!(validate_object_identity(42, id).is_ok());
    }
}
