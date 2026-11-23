//! Transactional publication and preservation of invalid component versions.

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(feature = "recorder-worker"))]
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::lease::ExclusiveComponentMutation;
use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, is_reparse_point, validate_relative_path,
};

#[cfg(not(feature = "recorder-worker"))]
mod cleanup;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use cleanup::{
    RecoveryEntry, clean, delete_empty_directory, delete_regular, list, purge_all_recorded,
    remove_empty_parents,
};

const SUFFIX: &str = ".recovery.json";
const MAX_ENTRIES: usize = 200_001;
const MAX_DEPTH: usize = 32;
const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_REASON_CHARS: usize = 2048;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[cfg(not(feature = "recorder-worker"))]
static AUTHENTICATED: LazyLock<Mutex<HashMap<PathBuf, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    id: String,
    version: String,
    directory: String,
    reason: String,
    files: Vec<Snapshot>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    path: PathBuf,
    size_bytes: u64,
    sha256: Option<String>,
    owned: bool,
    #[serde(default)]
    declared: bool,
    directory: bool,
}

/// `stage` must already be fully verified and contain its receipt. The caller
/// holds the registry mutex and an exclusive mutation for this component.
pub(super) fn publish_verified(
    id: &str,
    version: &str,
    expected: &[OwnedComponentFile],
    reason: &str,
    stage: &Path,
    exclusive: &ExclusiveComponentMutation,
) -> Result<()> {
    if !exclusive.matches(id) {
        bail!("component mutation reservation does not match the install target");
    }
    let target = super::component_version_root(id, version)?;
    let parent = super::ensure_component_parent(id)?;
    if target.parent() != Some(parent.as_path()) {
        bail!("component install target is unsafe");
    }
    match std::fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(metadata) => {
            if !metadata.is_dir() || is_reparse_point(&metadata) {
                bail!("invalid component version root is unsafe and was preserved");
            }
            quarantine(id, version, &target, expected, reason)?;
        }
    }
    no_replace_move(stage, &target)?;
    Ok(())
}

pub(super) fn no_replace_move(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        use windows::Win32::Storage::FileSystem::MoveFileW;
        use windows::core::PCWSTR;
        let from = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let to = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: both paths are NUL-terminated and remain valid for the call.
        unsafe { MoveFileW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()))? };
    }
    #[cfg(not(windows))]
    {
        if std::fs::symlink_metadata(target).is_ok() {
            bail!("component install target already exists");
        }
        std::fs::rename(source, target)?;
    }
    Ok(())
}

fn quarantine(
    id: &str,
    version: &str,
    source: &Path,
    expected: &[OwnedComponentFile],
    reason: &str,
) -> Result<()> {
    let mut owned = HashMap::new();
    for file in expected {
        validate_relative_path(&file.path)?;
        if owned.insert(file.path.clone(), file).is_some() {
            bail!("component recovery inventory repeats a path");
        }
    }
    let receipt_hash = validated_receipt_hash(source, id, version, &owned).ok();
    let mut inventory = Inventory {
        root: source,
        owned: &owned,
        receipt_hash: receipt_hash.as_deref(),
        files: Vec::new(),
        bytes: 0,
        count: 0,
    };
    inventory.walk(source, 0)?;
    let mut files = inventory.files;
    if !files
        .iter()
        .any(|file| file.path == Path::new(RECEIPT_NAME) && file.owned)
    {
        for file in &mut files {
            file.owned = false;
            file.declared = false;
        }
    }

    let root = recovery_root();
    let parent = root.join(id);
    ensure_directory(&root)?;
    ensure_directory(&parent)?;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory = format!("{version}-{timestamp}-{}-{sequence}", std::process::id());
    let target = parent.join(&directory);
    let sidecar = parent.join(format!("{directory}{SUFFIX}"));
    let record = Record {
        schema_version: 1,
        id: id.to_owned(),
        version: version.to_owned(),
        directory,
        reason: reason.chars().take(MAX_REASON_CHARS).collect(),
        files,
    };
    let body = serde_json::to_vec(&record)?;
    if body.len() as u64 > MAX_RECORD_BYTES {
        bail!("component recovery metadata exceeds its limit");
    }
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&sidecar)?;
    file.write_all(&body)?;
    file.sync_all()?;
    drop(file);
    // A sidecar with no directory remains visible if the move fails.
    no_replace_move(source, &target)?;
    #[cfg(not(feature = "recorder-worker"))]
    AUTHENTICATED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(sidecar, format!("{:x}", Sha256::digest(&body)));
    crate::log_info!(
        "[Components] preserved invalid {id} at {}",
        target.display()
    );
    Ok(())
}

struct Inventory<'a> {
    root: &'a Path,
    owned: &'a HashMap<PathBuf, &'a OwnedComponentFile>,
    receipt_hash: Option<&'a str>,
    files: Vec<Snapshot>,
    bytes: u64,
    count: usize,
}

impl Inventory<'_> {
    fn walk(&mut self, directory: &Path, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            bail!("component recovery tree is too deep");
        }
        for entry in std::fs::read_dir(directory)? {
            self.count += 1;
            if self.count > MAX_ENTRIES {
                bail!("component recovery has too many entries");
            }
            let path = entry?.path();
            let relative = path.strip_prefix(self.root)?.to_path_buf();
            validate_relative_path(&relative)?;
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.is_dir() && !is_reparse_point(&metadata) {
                self.files.push(Snapshot {
                    path: relative,
                    size_bytes: 0,
                    sha256: None,
                    owned: false,
                    declared: false,
                    directory: true,
                });
                self.walk(&path, depth + 1)?;
                continue;
            }
            let safe_file = metadata.is_file() && !is_reparse_point(&metadata);
            let sha256 = if safe_file {
                self.bytes = self
                    .bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| anyhow::anyhow!("component recovery is too large"))?;
                if self.bytes > MAX_SNAPSHOT_BYTES {
                    bail!("component recovery exceeds its size limit");
                }
                Some(hash_regular(&path, metadata.len())?)
            } else {
                None
            };
            let cleanable = if relative == Path::new(RECEIPT_NAME) {
                self.receipt_hash
                    .is_some_and(|expected| sha256.as_deref() == Some(expected))
            } else {
                self.owned.get(&relative).is_some_and(|expected| {
                    metadata.len() == expected.size_bytes
                        && sha256
                            .as_ref()
                            .is_some_and(|hash| hash.eq_ignore_ascii_case(&expected.sha256))
                })
            };
            self.files.push(Snapshot {
                owned: safe_file && self.receipt_hash.is_some() && cleanable,
                declared: safe_file
                    && self.receipt_hash.is_some()
                    && self.owned.contains_key(&relative),
                path: relative,
                size_bytes: metadata.len(),
                sha256,
                directory: false,
            });
        }
        Ok(())
    }
}

fn validated_receipt_hash(
    root: &Path,
    id: &str,
    version: &str,
    expected: &HashMap<PathBuf, &OwnedComponentFile>,
) -> Result<String> {
    let mut file = open_snapshot(&root.join(RECEIPT_NAME))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > 2 * 1024 * 1024 {
        bail!("component recovery receipt is unsafe");
    }
    let mut body = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut body)?;
    if body.len() as u64 != metadata.len() {
        bail!("component recovery receipt changed");
    }
    let receipt: ComponentReceipt = serde_json::from_slice(&body)?;
    receipt.validate()?;
    if receipt.id != id
        || receipt.version != version
        || receipt.files.len() != expected.len()
        || !receipt.files.iter().all(|file| {
            expected.get(&file.path).is_some_and(|entry| {
                file.size_bytes == entry.size_bytes
                    && file.sha256.eq_ignore_ascii_case(&entry.sha256)
            })
        })
    {
        bail!("component recovery receipt does not match the delivery");
    }
    Ok(format!("{:x}", Sha256::digest(&body)))
}

fn open_snapshot(path: &Path) -> Result<std::fs::File> {
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
    Ok(options.open(path)?)
}

pub(super) fn hash_regular(path: &Path, size: u64) -> Result<String> {
    let mut file = open_snapshot(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() != size {
        bail!("component recovery file changed while opening");
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if file.metadata()?.len() != size {
        bail!("component recovery file changed while hashing");
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn ensure_directory(path: &Path) -> Result<()> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component recovery directory is unsafe");
    }
    Ok(())
}

fn recovery_root() -> PathBuf {
    super::components_root()
        .parent()
        .expect("component root has a parent")
        .join("component-version-recovery")
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    #[test]
    fn failed_publish_preserves_quarantine_and_cleanup_preserves_modified_bytes() {
        let _mutation = super::super::acquire_mutation_guard().unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = format!("recovery-test-{unique}");
        let parent = super::super::ensure_component_parent(&id).unwrap();
        let old = parent.join("old");
        let selected = parent.join("new");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(old.join("data"), b"old").unwrap();
        std::fs::write(selected.join("data"), b"modified").unwrap();
        std::fs::write(selected.join("unknown"), b"user data").unwrap();
        std::fs::create_dir(selected.join("empty")).unwrap();
        let expected = [OwnedComponentFile {
            path: "data".into(),
            size_bytes: 8,
            sha256: format!("{:x}", Sha256::digest(b"expected")),
        }];
        super::super::write_receipt(
            &selected,
            &ComponentReceipt {
                schema_version: 1,
                id: id.clone(),
                version: "new".into(),
                architecture: "x64".into(),
                dependencies: Vec::new(),
                files: expected.to_vec(),
            },
        )
        .unwrap();
        let mut lease = super::super::acquire(&id).unwrap();
        let exclusive = super::super::lease::upgrade_to_exclusive(&mut lease).unwrap();
        let missing_stage = parent.join("missing-stage");
        assert!(
            publish_verified(&id, "new", &expected, "invalid", &missing_stage, &exclusive).is_err()
        );
        assert_eq!(std::fs::read(old.join("data")).unwrap(), b"old");
        assert!(!selected.exists());
        drop(exclusive);
        let entry = cleanup::list()
            .unwrap()
            .into_iter()
            .find(|entry| {
                entry
                    .path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    == Some(id.as_str())
            })
            .unwrap();
        assert!(entry.can_clean);
        let sidecar = entry.path.with_file_name(format!(
            "{}{}",
            entry.path.file_name().unwrap().to_str().unwrap(),
            SUFFIX
        ));
        let original = std::fs::read(&sidecar).unwrap();
        let record: Record = serde_json::from_slice(&original).unwrap();
        assert!(
            record
                .files
                .iter()
                .any(|file| file.path == Path::new("empty") && file.directory)
        );
        std::fs::write(&sidecar, b"tampered").unwrap();
        assert!(cleanup::clean(&entry).is_err());
        assert_eq!(
            std::fs::read(entry.path.join("unknown")).unwrap(),
            b"user data"
        );
        std::fs::write(&sidecar, &original).unwrap();
        let saved = AUTHENTICATED.lock().unwrap().remove(&sidecar).unwrap();
        assert!(cleanup::clean(&entry).is_err());
        assert_eq!(std::fs::read(entry.path.join("data")).unwrap(), b"modified");
        AUTHENTICATED.lock().unwrap().insert(sidecar.clone(), saved);
        let outcome = cleanup::clean(&entry).unwrap();
        assert!(!outcome.preserved_paths.is_empty());
        assert_eq!(std::fs::read(entry.path.join("data")).unwrap(), b"modified");
        assert_eq!(
            std::fs::read(entry.path.join("unknown")).unwrap(),
            b"user data"
        );
        assert!(entry.path.join("empty").is_dir());

        let outcome = cleanup::purge(&entry).unwrap();
        assert!(!outcome.preserved_paths.is_empty());
        assert!(!entry.path.join("data").exists());
        assert_eq!(
            std::fs::read(entry.path.join("unknown")).unwrap(),
            b"user data"
        );
        std::fs::remove_file(entry.path.join("unknown")).unwrap();
        std::fs::remove_dir(entry.path.join("empty")).unwrap();
        std::fs::remove_dir(&entry.path).unwrap();
        std::fs::remove_file(sidecar).unwrap();
        std::fs::remove_dir(entry.path.parent().unwrap()).unwrap();
        std::fs::remove_file(old.join("data")).unwrap();
        std::fs::remove_dir(old).unwrap();
        std::fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn move_does_not_replace_existing_target() {
        let root = std::env::temp_dir().join(format!(
            "sgt-move-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("source");
        let target = root.join("target");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("data"), b"existing").unwrap();
        assert!(no_replace_move(&source, &target).is_err());
        assert_eq!(std::fs::read(target.join("data")).unwrap(), b"existing");
        std::fs::remove_file(target.join("data")).unwrap();
        std::fs::remove_dir(target).unwrap();
        std::fs::remove_dir(source).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn missing_invalid_or_mismatched_receipt_never_authorizes_inventory() {
        let root = std::env::temp_dir().join(format!(
            "sgt-recovery-proof-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("payload"), b"expected").unwrap();
        let expected = OwnedComponentFile {
            path: "payload".into(),
            size_bytes: 8,
            sha256: format!("{:x}", Sha256::digest(b"expected")),
        };
        let owned = HashMap::from([(expected.path.clone(), &expected)]);
        let receipt = ComponentReceipt {
            schema_version: 1,
            id: "different-component".into(),
            version: "1.0.0".into(),
            architecture: "x64".into(),
            dependencies: Vec::new(),
            files: vec![expected.clone()],
        };
        for body in [
            None,
            Some(b"invalid".to_vec()),
            Some(serde_json::to_vec(&receipt).unwrap()),
        ] {
            if let Some(body) = body {
                std::fs::write(root.join(RECEIPT_NAME), body).unwrap();
            }
            assert!(validated_receipt_hash(&root, "expected-component", "1.0.0", &owned).is_err());
            let mut inventory = Inventory {
                root: &root,
                owned: &owned,
                receipt_hash: None,
                files: Vec::new(),
                bytes: 0,
                count: 0,
            };
            inventory.walk(&root, 0).unwrap();
            assert!(
                inventory
                    .files
                    .iter()
                    .all(|file| !file.owned && !file.declared)
            );
        }
        std::fs::remove_file(root.join(RECEIPT_NAME)).unwrap();
        std::fs::remove_file(root.join("payload")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
