use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::catalog::validate_identifier;

#[path = "../../build_support/component_path.rs"]
pub(super) mod component_path;

pub(super) const RECEIPT_NAME: &str = "receipt.json";
const MAX_FILES: usize = 100_000;
const MAX_RECEIPT_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentReceipt {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) architecture: String,
    pub(crate) dependencies: Vec<String>,
    pub(crate) files: Vec<OwnedComponentFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OwnedComponentFile {
    pub(crate) path: PathBuf,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: String,
}

impl ComponentReceipt {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.files.is_empty() || self.files.len() > MAX_FILES {
            bail!("component receipt shape is invalid");
        }
        validate_identifier(&self.id)?;
        validate_identifier(&self.version)?;
        validate_identifier(&self.architecture)?;
        if self.dependencies.len() > 32 {
            bail!("component receipt has too many dependencies");
        }
        let mut dependencies = std::collections::HashSet::new();
        for dependency in &self.dependencies {
            validate_identifier(dependency)?;
            if dependency == &self.id || !dependencies.insert(dependency) {
                bail!("component receipt contains an invalid dependency");
            }
        }
        let mut paths = std::collections::HashSet::new();
        for file in &self.files {
            validate_relative_path(&file.path)?;
            if !paths.insert(file.path.clone()) {
                bail!("component receipt contains duplicate paths");
            }
            if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                bail!("component receipt contains an invalid checksum");
            }
        }
        Ok(())
    }

    pub(crate) fn read(path: &Path) -> Result<Self> {
        #[cfg(not(windows))]
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            bail!("component receipt is not a regular file");
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            use windows::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            };
            options
                .share_mode(FILE_SHARE_READ.0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
        }
        let mut file = options
            .open(path)
            .with_context(|| format!("component receipt '{}' is missing", path.display()))?;
        Self::read_with_body(&mut file).map(|(receipt, _)| receipt)
    }

    pub(super) fn read_with_body(file: &mut std::fs::File) -> Result<(Self, Vec<u8>)> {
        let metadata = file.metadata()?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            bail!("component receipt is not a regular file");
        }
        if metadata.len() > MAX_RECEIPT_BYTES {
            bail!("component receipt exceeds its size limit");
        }
        let mut body = Vec::new();
        file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut body)?;
        if body.len() as u64 != metadata.len() || body.len() as u64 > MAX_RECEIPT_BYTES {
            bail!("component receipt changed while reading");
        }
        let receipt: Self =
            serde_json::from_slice(&body).context("component receipt is invalid")?;
        receipt.validate()?;
        Ok((receipt, body))
    }
}

pub(crate) fn write_receipt(root: &Path, receipt: &ComponentReceipt) -> Result<()> {
    receipt.validate()?;
    let mut body = ReceiptBody(Vec::new());
    serde_json::to_writer_pretty(&mut body, receipt)
        .context("component receipt serialization failed")?;
    std::fs::create_dir_all(root)?;
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component receipt root is not a regular directory");
    }
    crate::atomic_json::write_bytes_atomic(&root.join(RECEIPT_NAME), &body.0)?;
    Ok(())
}

struct ReceiptBody(Vec<u8>);

impl std::io::Write for ReceiptBody {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > MAX_RECEIPT_BYTES - self.0.len() as u64 {
            return Err(std::io::Error::other(
                "component receipt exceeds its size limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn validate_relative_path(path: &Path) -> Result<()> {
    if !path.to_str().is_some_and(component_path::valid) {
        bail!("component file path is not a safe relative Windows path");
    }
    Ok(())
}

/// Whether `path` is a regular file with the expected size and SHA-256. A file
/// this process already hashed and that is unchanged since is not re-read
/// (see `verified_cache`).
pub(super) fn file_matches(path: &Path, expected: &OwnedComponentFile) -> Result<bool> {
    if !file_size_matches(path, expected)? {
        super::verified_cache::forget(path);
        return Ok(false);
    }
    let mut file = std::fs::File::open(path)?;
    super::verified_cache::file_has_digest(path, &mut file, &expected.sha256)
}

/// Reads a delivered file once and returns exactly the bytes that were hashed,
/// so a file swapped after verification can never be served in its place.
/// Its callers (creation, web_assets) are host-only modules.
#[cfg(not(feature = "recorder-worker"))]
pub(super) fn read_verified_file(path: &Path, expected: &OwnedComponentFile) -> Result<Vec<u8>> {
    use sha2::{Digest as _, Sha256};

    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("delivered file is not a regular file");
    }
    let mut file = std::fs::File::open(path)?;
    let capacity = usize::try_from(expected.size_bytes).context("delivered file is too large")?;
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(expected.size_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() != capacity
        || !format!("{:x}", Sha256::digest(&bytes)).eq_ignore_ascii_case(&expected.sha256)
    {
        bail!("delivered file failed integrity verification");
    }
    Ok(bytes)
}

/// Cheap display/status probe. This never grants execution authority: callers
/// must still use [`file_matches`] while holding their component lease/handles
/// before loading or launching delivered bytes.
pub(super) fn file_size_matches(path: &Path, expected: &OwnedComponentFile) -> Result<bool> {
    let metadata = std::fs::symlink_metadata(path)?;
    Ok(metadata.is_file() && !is_reparse_point(&metadata) && metadata.len() == expected.size_bytes)
}

pub(super) fn resolve_owned_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    let full_path = root.join(relative);
    let Some(parent) = full_path.parent() else {
        bail!("component file path has no parent");
    };
    let Ok(parent_relative) = parent.strip_prefix(root) else {
        bail!("component file path escaped its version root");
    };
    let mut current = root.to_path_buf();
    for component in parent_relative.components() {
        let Component::Normal(name) = component else {
            bail!("component file parent is unsafe");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {}
            Ok(_) => bail!("component file parent is not a regular directory"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(full_path)
}

#[cfg(windows)]
pub(crate) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(crate) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "recorder-worker"))]
    use super::read_verified_file;
    use super::{
        ComponentReceipt, MAX_FILES, MAX_RECEIPT_BYTES, OwnedComponentFile, ReceiptBody,
        file_matches, file_size_matches, write_receipt,
    };
    use sha2::{Digest as _, Sha256};
    use std::io::Write as _;

    #[test]
    fn relative_paths_share_delivery_utf16_budgets() {
        for path in [
            format!("{}/{}", "界".repeat(200), "界".repeat(200)),
            format!("{}/{}", "🦀".repeat(127), "🦀".repeat(127)),
        ] {
            assert!(path.len() > 512);
            assert!(super::component_path::valid(&path));
            super::validate_relative_path(std::path::Path::new(&path)).unwrap();
        }
        for path in [
            format!("{}/file", "🦀".repeat(128)),
            format!("{}/{}/xxx", "a".repeat(254), "b".repeat(254)),
        ] {
            assert!(!super::component_path::valid(&path));
            assert!(super::validate_relative_path(std::path::Path::new(&path)).is_err());
        }
    }

    fn inventory(count: usize) -> ComponentReceipt {
        ComponentReceipt {
            schema_version: 1,
            id: "test-bounded-inventory".into(),
            version: "1.0.0".into(),
            architecture: "x64".into(),
            dependencies: Vec::new(),
            files: (0..count)
                .map(|index| OwnedComponentFile {
                    path: format!("packages/module{index}/source.py").into(),
                    size_bytes: 1,
                    sha256: "a".repeat(64),
                })
                .collect(),
        }
    }

    #[test]
    fn file_count_remains_bounded() {
        let mut receipt = inventory(MAX_FILES);
        receipt.validate().unwrap();
        receipt.files.push(receipt.files[0].clone());
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn serialization_limit_is_inclusive_and_failure_preserves_buffer() {
        let mut body = ReceiptBody(vec![0; MAX_RECEIPT_BYTES as usize - 1]);
        body.write_all(b"x").unwrap();
        assert_eq!(body.0.len() as u64, MAX_RECEIPT_BYTES);
        assert!(body.write_all(b"y").is_err());
        assert_eq!(body.0.last(), Some(&b'x'));
        assert_eq!(body.0.len() as u64, MAX_RECEIPT_BYTES);
    }

    #[test]
    fn oversized_serialization_preserves_receipt_and_does_not_create_root() {
        let root = std::env::temp_dir().join(format!(
            "sgt-receipt-size-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let original = inventory(1);
        write_receipt(&root, &original).unwrap();
        let path = root.join("receipt.json");
        let before = std::fs::read(&path).unwrap();
        let mut oversized = inventory(60_000);
        let padding = "x".repeat(240);
        for (index, file) in oversized.files.iter_mut().enumerate() {
            file.path = format!("{padding}/{padding}/{index}.py").into();
        }
        oversized.validate().unwrap();
        assert!(write_receipt(&root, &oversized).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let absent = root.join("absent");
        assert!(write_receipt(&absent, &oversized).is_err());
        assert!(!absent.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn reader_rejects_oversized_file_before_parsing() {
        let root = std::env::temp_dir().join(format!(
            "sgt-receipt-read-size-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("receipt.json");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.set_len(MAX_RECEIPT_BYTES + 1).unwrap();
        drop(file);
        assert!(
            ComponentReceipt::read(&path)
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn display_probe_never_replaces_launch_integrity() {
        let root = std::env::temp_dir().join(format!(
            "sgt-receipt-status-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("payload.bin");
        std::fs::write(&path, b"evil").unwrap();
        let expected = OwnedComponentFile {
            path: "payload.bin".into(),
            size_bytes: 4,
            sha256: format!("{:x}", Sha256::digest(b"good")),
        };

        assert!(file_size_matches(&path, &expected).unwrap());
        assert!(!file_matches(&path, &expected).unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[cfg(not(feature = "recorder-worker"))]
    fn verified_read_returns_only_the_bytes_it_hashed() {
        let root = std::env::temp_dir().join(format!(
            "sgt-receipt-read-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("index.html");
        let expected = OwnedComponentFile {
            path: "index.html".into(),
            size_bytes: 4,
            sha256: format!("{:x}", Sha256::digest(b"good")),
        };

        std::fs::write(&path, b"good").unwrap();
        assert_eq!(read_verified_file(&path, &expected).unwrap(), b"good");
        std::fs::write(&path, b"evil").unwrap();
        assert!(read_verified_file(&path, &expected).is_err());
        std::fs::write(&path, b"good-and-more").unwrap();
        assert!(read_verified_file(&path, &expected).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
