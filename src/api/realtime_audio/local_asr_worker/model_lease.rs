use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, bail};
use sgt_local_asr_protocol::Mode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelKind {
    RealtimeEou,
    SubtitleTdt,
}

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelRemovalOutcome {
    Removed,
    Pending,
}

#[derive(Default)]
struct LeaseState {
    leases: usize,
    pending_root: Option<PathBuf>,
    removing: bool,
    notice: Option<String>,
}

static STATES: LazyLock<[Mutex<LeaseState>; 2]> = LazyLock::new(|| {
    [
        Mutex::new(LeaseState::default()),
        Mutex::new(LeaseState::default()),
    ]
});

pub(super) struct ModelLease {
    kind: ModelKind,
    contracts: &'static [super::super::model_loader::FileContract],
    _files: Vec<std::fs::File>,
}

impl ModelLease {
    pub(super) fn acquire(mode: Mode, requested_root: &Path) -> Result<Self> {
        let kind = match mode {
            Mode::RealtimeEou => ModelKind::RealtimeEou,
            Mode::SubtitleTdt => ModelKind::SubtitleTdt,
        };
        validate_expected_root(kind, requested_root)?;
        let contracts = contracts(kind);
        {
            let mut state = state(kind)
                .lock()
                .unwrap_or_else(|value| value.into_inner());
            if state.pending_root.is_some() || state.removing {
                bail!("local ASR model removal is pending");
            }
            state.leases = state
                .leases
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("local ASR model lease count overflow"))?;
        }
        let mut lease = Self {
            kind,
            contracts,
            _files: Vec::new(),
        };
        lease._files = open_verified_model_files(requested_root, contracts)?;
        Ok(lease)
    }
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        let pending = {
            let mut state = state(self.kind)
                .lock()
                .unwrap_or_else(|value| value.into_inner());
            state.leases = state.leases.saturating_sub(1);
            if state.leases == 0 {
                let pending = state.pending_root.take();
                state.removing = pending.is_some();
                pending
            } else {
                None
            }
        };
        self._files.clear();
        if let Some(root) = pending {
            let notice = remove_owned_model_files(&root, self.contracts)
                .err()
                .map(|error| error.to_string());
            let mut state = state(self.kind)
                .lock()
                .unwrap_or_else(|value| value.into_inner());
            state.notice = notice;
            state.removing = false;
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn request_remove(kind: ModelKind, root: &Path) -> Result<ModelRemovalOutcome> {
    validate_requested_location(kind, root)?;
    let mut guard = state(kind)
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if guard.removing {
        guard.notice = Some("Local ASR model removal is already finishing.".to_string());
        return Ok(ModelRemovalOutcome::Pending);
    }
    if guard.leases > 0 {
        guard.pending_root = Some(root.to_path_buf());
        guard.notice =
            Some("Removal will finish after the active local transcription stops.".to_string());
        return Ok(ModelRemovalOutcome::Pending);
    }
    guard.removing = true;
    drop(guard);
    let result = remove_owned_model_files(root, contracts(kind));
    let mut state = state(kind)
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    state.removing = false;
    if let Err(error) = result {
        state.notice = Some(error.to_string());
        return Err(error);
    }
    state.notice = None;
    Ok(ModelRemovalOutcome::Removed)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn current_notice(kind: ModelKind) -> Option<String> {
    state(kind)
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .notice
        .clone()
}

fn validate_expected_root(kind: ModelKind, root: &Path) -> Result<()> {
    validate_requested_location(kind, root)?;
    let metadata = std::fs::symlink_metadata(root)
        .with_context(|| format!("inspect local ASR model root '{}'", root.display()))?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("local ASR model root is unsafe");
    }
    Ok(())
}

fn validate_requested_location(kind: ModelKind, root: &Path) -> Result<()> {
    let expected = match kind {
        ModelKind::RealtimeEou => super::super::model_loader::get_parakeet_model_dir(),
        ModelKind::SubtitleTdt => super::super::parakeet_tdt_assets::get_parakeet_tdt_model_dir(),
    };
    if path_key(root) != path_key(&expected) {
        bail!("local ASR model path is outside its managed location");
    }
    Ok(())
}

fn contracts(kind: ModelKind) -> &'static [super::super::model_loader::FileContract] {
    match kind {
        ModelKind::RealtimeEou => super::super::model_loader::parakeet_model_contracts(),
        ModelKind::SubtitleTdt => super::super::parakeet_tdt_assets::parakeet_tdt_model_contracts(),
    }
}

fn open_verified_model_files(
    root: &Path,
    contracts: &[super::super::model_loader::FileContract],
) -> Result<Vec<std::fs::File>> {
    let mut files = Vec::with_capacity(contracts.len());
    for contract in contracts {
        let path = root.join(contract.name);
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("inspect local ASR model file '{}'", path.display()))?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            bail!("local ASR model file '{}' is unsafe", path.display());
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
        options.share_mode(FILE_SHARE_READ.0);
        let mut file = options
            .open(&path)
            .with_context(|| format!("lock local ASR model file '{}'", path.display()))?;
        let locked_metadata = file.metadata()?;
        if !locked_metadata.is_file() || locked_metadata.len() != contract.size_bytes {
            bail!(
                "local ASR model file '{}' has an invalid size",
                path.display()
            );
        }
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            use sha2::Digest as _;
            hasher.update(&buffer[..read]);
        }
        use sha2::Digest as _;
        if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(contract.sha256) {
            bail!(
                "local ASR model file '{}' failed integrity verification",
                path.display()
            );
        }
        file.seek(SeekFrom::Start(0))?;
        files.push(file);
    }
    Ok(files)
}

fn remove_owned_model_files(
    root: &Path,
    contracts: &[super::super::model_loader::FileContract],
) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("local ASR model root is unsafe; files were preserved");
    }
    remove_verified_files(root, contracts)
}

fn remove_verified_files(
    root: &Path,
    contracts: &[super::super::model_loader::FileContract],
) -> Result<()> {
    let paths = contracts
        .iter()
        .map(|contract| (root.join(contract.name), *contract))
        .collect::<Vec<_>>();
    for (path, contract) in &paths {
        match std::fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.is_file()
                    && !is_reparse_point(&metadata)
                    && super::super::model_loader::verified_file_present(path, *contract) => {}
            Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {
                bail!("modified model file '{}' was preserved", path.display())
            }
            Ok(_) => bail!("unsafe model entry '{}' was preserved", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    for (path, _) in paths {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {
                std::fs::remove_file(path)?;
            }
            Ok(_) => bail!("unsafe model entry '{}' was preserved", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let _ = std::fs::remove_dir(root);
    Ok(())
}

fn state(kind: ModelKind) -> &'static Mutex<LeaseState> {
    &STATES[match kind {
        ModelKind::RealtimeEou => 0,
        ModelKind::SubtitleTdt => 1,
    }]
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONTRACTS: &[super::super::super::model_loader::FileContract] =
        &[super::super::super::model_loader::FileContract {
            name: "managed.bin",
            url: "https://example.invalid/managed.bin",
            size_bytes: 8,
            sha256: "cea23dd4b87e8b00d19fb9ccaaef93e97353c7353e2070f3baf05aeb3995dff4",
        }];

    #[test]
    fn managed_cleanup_preserves_unknown_model_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-model-cleanup-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("managed.bin"), b"expected").unwrap();
        std::fs::write(root.join("user-note.txt"), b"preserve").unwrap();
        remove_verified_files(&root, TEST_CONTRACTS).unwrap();
        assert!(!root.join("managed.bin").exists());
        assert_eq!(
            std::fs::read(root.join("user-note.txt")).unwrap(),
            b"preserve"
        );
        std::fs::remove_file(root.join("user-note.txt")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn managed_cleanup_preserves_modified_model_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-model-modified-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("managed.bin"), b"modified").unwrap();
        assert!(remove_verified_files(&root, TEST_CONTRACTS).is_err());
        assert_eq!(
            std::fs::read(root.join("managed.bin")).unwrap(),
            b"modified"
        );
        std::fs::remove_file(root.join("managed.bin")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn last_lease_drop_completes_pending_removal_after_unlocking_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-model-pending-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("managed.bin"), b"expected").unwrap();
        let files = open_verified_model_files(&root, TEST_CONTRACTS).unwrap();
        {
            let mut state = state(ModelKind::RealtimeEou)
                .lock()
                .unwrap_or_else(|value| value.into_inner());
            *state = LeaseState {
                leases: 1,
                pending_root: Some(root.clone()),
                removing: false,
                notice: None,
            };
        }
        drop(ModelLease {
            kind: ModelKind::RealtimeEou,
            contracts: TEST_CONTRACTS,
            _files: files,
        });
        assert!(!root.join("managed.bin").exists());
        assert!(!root.exists());
        let state = state(ModelKind::RealtimeEou)
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        assert_eq!(state.leases, 0);
        assert!(!state.removing);
        assert!(state.notice.is_none());
    }
}
