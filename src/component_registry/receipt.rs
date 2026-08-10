use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::catalog::validate_identifier;

pub(super) const RECEIPT_NAME: &str = "receipt.json";
const MAX_FILES: usize = 4096;
const MAX_RECEIPT_BYTES: u64 = 2 * 1024 * 1024;

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
        let metadata = std::fs::symlink_metadata(path)
            .with_context(|| format!("component receipt '{}' is missing", path.display()))?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            bail!("component receipt is not a regular file");
        }
        let file = std::fs::File::open(path)
            .with_context(|| format!("component receipt '{}' is missing", path.display()))?;
        if metadata.len() > MAX_RECEIPT_BYTES {
            bail!("component receipt exceeds its size limit");
        }
        let mut body = String::new();
        file.take(MAX_RECEIPT_BYTES + 1).read_to_string(&mut body)?;
        let receipt: Self = serde_json::from_str(&body).context("component receipt is invalid")?;
        receipt.validate()?;
        Ok(receipt)
    }
}

pub(crate) fn write_receipt(root: &Path, receipt: &ComponentReceipt) -> Result<()> {
    receipt.validate()?;
    std::fs::create_dir_all(root)?;
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component receipt root is not a regular directory");
    }
    crate::atomic_json::write_json_atomic(&root.join(RECEIPT_NAME), receipt)?;
    Ok(())
}

pub(super) fn validate_relative_path(path: &Path) -> Result<()> {
    let mut count = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => count += 1,
            _ => bail!("component file path is unsafe"),
        }
    }
    if count == 0 || count > 32 || path.as_os_str().len() > 512 {
        bail!("component file path is invalid");
    }
    Ok(())
}

pub(super) fn file_matches(path: &Path, expected: &OwnedComponentFile) -> Result<bool> {
    if !file_size_matches(path, expected)? {
        return Ok(false);
    }
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected.sha256))
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
pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::{OwnedComponentFile, file_matches, file_size_matches};
    use sha2::{Digest as _, Sha256};

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
}
