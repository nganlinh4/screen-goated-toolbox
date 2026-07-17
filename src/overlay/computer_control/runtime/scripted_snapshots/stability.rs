//! Identity-bound source locks and authenticated staged artifact handles.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, ensure};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO, FILE_ID_INFO, FILE_SHARE_READ, FileBasicInfo,
    FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
};

const SETTLE_INTERVAL: Duration = Duration::from_millis(250);

pub(super) struct ConfiguredSource {
    path: PathBuf,
}

impl ConfiguredSource {
    pub(super) fn configure(path: PathBuf) -> anyhow::Result<Self> {
        ensure!(path.is_absolute(), "snapshot source must be absolute");
        let canonical = std::fs::canonicalize(&path)
            .with_context(|| format!("snapshot source could not be canonicalized: {path:?}"))?;
        ensure!(canonical.is_file(), "snapshot source must be a file");
        Ok(Self { path: canonical })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug)]
pub(super) struct SnapshotEvidence {
    pub(super) bytes: u64,
    pub(super) sha256: String,
    source: SourceSample,
}

#[derive(Debug)]
pub(super) struct StabilityError {
    pub(super) reason: &'static str,
    detail: String,
}

impl StabilityError {
    fn new(reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }

    fn io(reason: &'static str, error: impl fmt::Display) -> Self {
        Self::new(reason, error.to_string())
    }
}

impl fmt::Display for StabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.reason, self.detail)
    }
}

impl std::error::Error for StabilityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    volume: u64,
    id: [u8; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileFingerprint {
    identity: FileIdentity,
    bytes: u64,
    creation_time: i64,
    last_write_time: i64,
    change_time: i64,
    attributes: u32,
    links: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SourceSample {
    fingerprint: FileFingerprint,
    sha256: [u8; 32],
}

pub(super) fn probe_sources(
    sources: &[ConfiguredSource],
) -> Result<Vec<SourceSample>, StabilityError> {
    sources
        .iter()
        .map(|source| sample_path(source.path()))
        .collect()
}

pub(super) fn settle_sources() {
    std::thread::sleep(SETTLE_INTERVAL);
}

pub(super) struct LockedSource<'a> {
    configured: &'a ConfiguredSource,
    baseline: SourceSample,
    file: File,
}

impl<'a> LockedSource<'a> {
    pub(super) fn configured_path(&self) -> &Path {
        self.configured.path()
    }

    pub(super) fn acquire_all(
        sources: &'a [ConfiguredSource],
        baselines: &[SourceSample],
    ) -> Result<Vec<Self>, StabilityError> {
        if sources.len() != baselines.len() {
            return Err(StabilityError::new(
                "source_set_changed",
                "configured source and baseline counts differ",
            ));
        }
        let mut locked = Vec::with_capacity(sources.len());
        for (configured, baseline) in sources.iter().zip(baselines) {
            let file = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ.0)
                .open(configured.path())
                .map_err(|error| StabilityError::io("source_lock_failed", error))?;
            locked.push(Self {
                configured,
                baseline: *baseline,
                file,
            });
        }
        for source in &mut locked {
            source.validate_bound_path()?;
        }
        Ok(locked)
    }

    pub(super) fn stage(
        &mut self,
        staged_destination: &Path,
    ) -> Result<(StagedArtifact, SnapshotEvidence), StabilityError> {
        let mut staged = StagedArtifact::create(staged_destination)?;
        let sample = sample_handle(&mut self.file, Some(staged.file_mut()))?;
        compare_samples(
            self.baseline,
            sample,
            "between stability probes",
            SampleKind::Source,
        )?;
        staged.seal(sample)?;
        Ok((
            staged,
            SnapshotEvidence {
                bytes: sample.fingerprint.bytes,
                sha256: hex_hash(&sample.sha256),
                source: sample,
            },
        ))
    }

    pub(super) fn authenticate(
        &mut self,
        evidence: &SnapshotEvidence,
    ) -> Result<(), StabilityError> {
        let current = sample_handle(&mut self.file, None)?;
        compare_samples(
            evidence.source,
            current,
            "while the complete source set was locked for publication",
            SampleKind::Source,
        )?;
        self.validate_path_identity(current.fingerprint.identity)
    }

    fn validate_bound_path(&mut self) -> Result<(), StabilityError> {
        let locked = sample_handle(&mut self.file, None)?;
        compare_samples(
            self.baseline,
            locked,
            "before the complete source set was locked",
            SampleKind::Source,
        )?;
        self.validate_path_identity(locked.fingerprint.identity)
    }

    fn validate_path_identity(&self, expected: FileIdentity) -> Result<(), StabilityError> {
        let path_identity = fingerprint(
            &File::open(self.configured.path())
                .map_err(|error| StabilityError::io("source_path_reopen_failed", error))?,
        )?
        .identity;
        if path_identity != expected {
            return Err(StabilityError::new(
                "source_path_identity_changed",
                "configured path no longer names the retained source handle",
            ));
        }
        Ok(())
    }
}

pub(super) struct StagedArtifact {
    path: PathBuf,
    file: File,
    sealed: Option<SourceSample>,
}

#[derive(Clone, Copy)]
pub(super) struct ArtifactProof {
    sample: SourceSample,
}

impl StagedArtifact {
    fn create(path: &Path) -> Result<Self, StabilityError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(path)
            .map_err(|error| StabilityError::io("staged_create_failed", error))?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            sealed: None,
        })
    }

    fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    fn seal(&mut self, source: SourceSample) -> Result<(), StabilityError> {
        self.file
            .sync_all()
            .map_err(|error| StabilityError::io("staged_sync_failed", error))?;
        let staged = sample_handle(&mut self.file, None)?;
        if staged.fingerprint.bytes != source.fingerprint.bytes || staged.sha256 != source.sha256 {
            return Err(StabilityError::new(
                "staged_content_mismatch",
                "staged bytes do not exactly match the locked source",
            ));
        }
        self.sealed = Some(staged);
        Ok(())
    }

    pub(super) fn authenticate(&mut self) -> Result<(), StabilityError> {
        let expected = self.sealed.ok_or_else(|| {
            StabilityError::new("staged_not_sealed", "staged artifact has no trusted sample")
        })?;
        let current = sample_handle(&mut self.file, None)?;
        compare_samples(
            expected,
            current,
            "while retained for atomic publication",
            SampleKind::Staged,
        )?;
        let path = File::open(&self.path)
            .map_err(|error| StabilityError::io("staged_path_reopen_failed", error))?;
        if fingerprint(&path)?.identity != current.fingerprint.identity {
            return Err(StabilityError::new(
                "staged_path_identity_changed",
                "staged path no longer names the retained artifact handle",
            ));
        }
        Ok(())
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn proof(&self) -> Result<ArtifactProof, StabilityError> {
        self.sealed
            .map(|sample| ArtifactProof { sample })
            .ok_or_else(|| {
                StabilityError::new("staged_not_sealed", "staged artifact has no trusted sample")
            })
    }
}

pub(super) struct PublishedArtifact {
    path: PathBuf,
    file: File,
    expected: ArtifactProof,
}

impl PublishedArtifact {
    pub(super) fn acquire(path: PathBuf, expected: ArtifactProof) -> Result<Self, StabilityError> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&path)
            .map_err(|error| StabilityError::io("published_lock_failed", error))?;
        Ok(Self {
            path,
            file,
            expected,
        })
    }

    pub(super) fn authenticate(&mut self) -> Result<(), StabilityError> {
        let current = sample_handle(&mut self.file, None)?;
        compare_samples(
            self.expected.sample,
            current,
            "after no-replace atomic publication",
            SampleKind::Published,
        )?;
        let path = File::open(&self.path)
            .map_err(|error| StabilityError::io("published_path_reopen_failed", error))?;
        if file_identity(&path)? != current.fingerprint.identity {
            return Err(StabilityError::new(
                "published_path_identity_changed",
                "published path no longer names the retained artifact handle",
            ));
        }
        Ok(())
    }
}

fn sample_path(path: &Path) -> Result<SourceSample, StabilityError> {
    let mut file =
        File::open(path).map_err(|error| StabilityError::io("source_open_failed", error))?;
    sample_handle(&mut file, None)
}

fn sample_handle(
    file: &mut File,
    mut staged_output: Option<&mut File>,
) -> Result<SourceSample, StabilityError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| StabilityError::io("source_seek_failed", error))?;
    let before = fingerprint(file)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| StabilityError::io("source_read_failed", error))?;
        if read == 0 {
            break;
        }
        if let Some(output) = staged_output.as_deref_mut() {
            output
                .write_all(&buffer[..read])
                .map_err(|error| StabilityError::io("staged_write_failed", error))?;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or_else(|| StabilityError::new("source_too_large", "byte count overflowed"))?;
    }
    let after = fingerprint(file)?;
    if before != after || bytes != after.bytes {
        return Err(StabilityError::new(
            "file_metadata_changed_during_read",
            "file metadata changed while its complete content was read",
        ));
    }
    Ok(SourceSample {
        fingerprint: after,
        sha256: hasher.finalize().into(),
    })
}

fn compare_samples(
    expected: SourceSample,
    current: SourceSample,
    boundary: &str,
    kind: SampleKind,
) -> Result<(), StabilityError> {
    let (identity_reason, metadata_reason, content_reason) = match kind {
        SampleKind::Source => (
            "source_identity_changed",
            "source_metadata_changed",
            "source_content_changed",
        ),
        SampleKind::Staged => (
            "staged_identity_changed",
            "staged_metadata_changed",
            "staged_content_changed",
        ),
        SampleKind::Published => (
            "published_identity_changed",
            "published_metadata_changed",
            "published_content_changed",
        ),
    };
    if expected.fingerprint.identity != current.fingerprint.identity {
        return Err(StabilityError::new(
            identity_reason,
            format!("file identity changed {boundary}"),
        ));
    }
    if expected.fingerprint != current.fingerprint {
        return Err(StabilityError::new(
            metadata_reason,
            format!("file metadata changed {boundary}"),
        ));
    }
    if expected.sha256 != current.sha256 {
        return Err(StabilityError::new(
            content_reason,
            format!("full file hash changed {boundary}"),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum SampleKind {
    Source,
    Staged,
    Published,
}

fn fingerprint(file: &File) -> Result<FileFingerprint, StabilityError> {
    let metadata = file
        .metadata()
        .map_err(|error| StabilityError::io("file_metadata_failed", error))?;
    if !metadata.is_file() {
        return Err(StabilityError::new(
            "file_not_regular",
            "snapshot object is no longer a regular file",
        ));
    }
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the borrowed File owns a valid handle and the output lives through the call.
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information)
            .map_err(|error| StabilityError::io("file_metadata_unavailable", error))?;
    }
    let basic = basic_information(file)?;
    Ok(FileFingerprint {
        identity: file_identity(file)?,
        bytes: u64::from(information.nFileSizeHigh) << 32 | u64::from(information.nFileSizeLow),
        creation_time: basic.CreationTime,
        last_write_time: basic.LastWriteTime,
        change_time: basic.ChangeTime,
        attributes: basic.FileAttributes,
        links: information.nNumberOfLinks,
    })
}

fn file_identity(file: &File) -> Result<FileIdentity, StabilityError> {
    let mut information = FILE_ID_INFO::default();
    // SAFETY: the File owns a valid handle and the correctly-sized output lives through the call.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
        .map_err(|error| StabilityError::io("file_identity_unavailable", error))?;
    }
    validate_file_identity(
        information.VolumeSerialNumber,
        information.FileId.Identifier,
    )
}

fn validate_file_identity(volume: u64, id: [u8; 16]) -> Result<FileIdentity, StabilityError> {
    if id.iter().all(|byte| *byte == 0) || id.iter().all(|byte| *byte == u8::MAX) {
        return Err(StabilityError::new(
            "file_identity_unsupported",
            "filesystem returned an unavailable 128-bit file identity sentinel",
        ));
    }
    Ok(FileIdentity { volume, id })
}

fn basic_information(file: &File) -> Result<FILE_BASIC_INFO, StabilityError> {
    let mut information = FILE_BASIC_INFO::default();
    // SAFETY: the File owns a valid handle and the correctly-sized output lives through the call.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileBasicInfo,
            (&mut information as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
        .map_err(|error| StabilityError::io("file_metadata_unavailable", error))?;
    }
    Ok(information)
}

fn hex_hash(hash: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in hash {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::validate_file_identity;

    #[test]
    fn unavailable_extended_file_identity_sentinels_fail_closed() {
        for id in [[0; 16], [u8::MAX; 16]] {
            let error = validate_file_identity(42, id).unwrap_err();
            assert_eq!(error.reason, "file_identity_unsupported");
        }
    }

    #[test]
    fn nonzero_extended_file_identity_is_accepted() {
        let mut id = [0; 16];
        id[15] = 1;
        assert!(validate_file_identity(42, id).is_ok());
    }
}
