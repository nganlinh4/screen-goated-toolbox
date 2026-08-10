//! Immutable model delivery and active-use ownership guards.

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, is_reparse_point, resolve_owned_path,
};
use super::{ComponentLease, RemovalOutcome};

mod auxiliary;
mod install;
mod staging;
#[cfg(not(feature = "recorder-worker"))]
mod update;

const DELIVERY_JSON: &str = include_str!("../../../model-delivery/windows-v1.json");
const MAX_MODELS: usize = 32;
const MAX_MODEL_FILES: usize = 4_096;
const MAX_MODEL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const ARCHITECTURE: &str = "any";

fn state_root() -> PathBuf {
    #[cfg(test)]
    return std::env::temp_dir().join(format!(
        "screen-goated-toolbox-model-state-tests-{}",
        std::process::id()
    ));
    #[cfg(not(test))]
    crate::paths::app_runtime_local_data_dir()
}

#[derive(Clone, Eq, PartialEq)]
struct StatusStamp {
    root_modified: Option<SystemTime>,
    receipt_bytes: u64,
    receipt_modified: Option<SystemTime>,
}

struct CachedStatus {
    version: String,
    stamp: StatusStamp,
    valid: bool,
}

static STATUS_CACHE: LazyLock<Mutex<HashMap<String, CachedStatus>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelKind {
    QwenSmall,
    QwenLarge,
    StepAudio,
    Magpie,
    Kokoro,
    Supertonic,
    Vieneu,
}

impl ModelKind {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::QwenSmall => "qwen3-asr-0-6b-model",
            Self::QwenLarge => "qwen3-asr-1-7b-model",
            Self::StepAudio => "step-audio-editx-model",
            Self::Magpie => "magpie-multilingual-357m-model",
            Self::Kokoro => "kokoro-82m-v1-model",
            Self::Supertonic => "supertonic-3-model",
            Self::Vieneu => "vieneu-v2-turbo-model",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeliveryCatalog {
    schema_version: u32,
    models: Vec<ModelDelivery>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelDelivery {
    id: String,
    version: String,
    architecture: String,
    #[serde(default)]
    archive: Option<ModelArchive>,
    installed_size_bytes: u64,
    files: Vec<ModelFile>,
    #[serde(default)]
    legacy_root: Option<LegacyRoot>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelArchive {
    url: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelFile {
    path: PathBuf,
    #[serde(default)]
    url: Option<String>,
    size_bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyRoot {
    kind: LegacyRootKind,
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum LegacyRootKind {
    RoamingModels,
    LocalModels,
}

pub(crate) struct ModelUse {
    root: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl ModelUse {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

pub(crate) fn model_dir(kind: ModelKind) -> PathBuf {
    let delivery = delivery(kind).expect("embedded model delivery is valid");
    super::component_version_root(&delivery.id, &delivery.version)
        .expect("embedded model identifiers are valid")
}

pub(crate) fn is_installed(kind: ModelKind) -> bool {
    validate_status(delivery(kind).expect("embedded model delivery is valid")).is_ok()
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn installed_size(kind: ModelKind) -> u64 {
    let delivery = delivery(kind).expect("embedded model delivery is valid");
    if validate_status(delivery).is_ok() {
        delivery.installed_size_bytes
    } else {
        0
    }
}

pub(crate) fn finish_auxiliary_removal(
    id: &str,
    outcome: RemovalOutcome,
    mutation: &super::RegistryMutationGuard,
) -> Result<RemovalOutcome> {
    auxiliary::finish_removal(id, outcome, mutation)
}

pub(crate) fn ensure(
    kind: ModelKind,
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<ModelUse> {
    let delivery = delivery(kind)?;
    install::ensure(delivery, cancelled, on_progress)?;
    acquire_delivery(delivery)
}

pub(crate) fn acquire_installed(kind: ModelKind) -> Result<ModelUse> {
    acquire_delivery(delivery(kind)?)
}

pub(crate) fn acquire_for_path(path: &Path) -> Result<ModelUse> {
    for kind in [ModelKind::QwenSmall, ModelKind::QwenLarge] {
        if path == model_dir(kind) {
            return acquire_installed(kind);
        }
    }
    bail!("model path is not owned by this build")
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn remove(kind: ModelKind) -> Result<RemovalOutcome> {
    let outcome = super::request_remove(kind.id())?;
    invalidate_status(kind.id());
    Ok(outcome)
}

fn acquire_delivery(delivery: &ModelDelivery) -> Result<ModelUse> {
    let lease = super::acquire(&delivery.id)?;
    let root = version_root(delivery)?;
    let result = (|| {
        validate_status(delivery)?;
        let files = lock_and_verify_files(&root, &delivery.files)?;
        #[cfg(test)]
        run_post_hash_test_hook();
        validate_exact_tree(&root, &delivery.files)?;
        Ok(files)
    })();
    match result {
        Ok(files) => Ok(ModelUse {
            root,
            _files: files,
            _lease: lease,
        }),
        Err(error) => {
            cache_invalid_status(delivery, &root);
            Err(error)
        }
    }
}

fn catalog() -> &'static DeliveryCatalog {
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(catalog) = update::catalog() {
        return catalog;
    }
    embedded_delivery_catalog()
}

fn embedded_delivery_catalog() -> &'static DeliveryCatalog {
    static CATALOG: OnceLock<DeliveryCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let catalog: DeliveryCatalog =
            serde_json::from_str(DELIVERY_JSON).expect("embedded model delivery is invalid JSON");
        catalog
            .validate()
            .expect("embedded model delivery violates its contract");
        catalog
    })
}

fn delivery(kind: ModelKind) -> Result<&'static ModelDelivery> {
    catalog()
        .models
        .iter()
        .find(|delivery| delivery.id == kind.id())
        .ok_or_else(|| anyhow!("{} has no delivery in this build", kind.id()))
}

impl DeliveryCatalog {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.models.is_empty() || self.models.len() > MAX_MODELS {
            bail!("model delivery catalog shape is invalid");
        }
        let component_catalog = super::embedded_catalog();
        let mut ids = HashSet::new();
        for delivery in &self.models {
            super::catalog::validate_identifier(&delivery.id)?;
            super::catalog::validate_identifier(&delivery.version)?;
            super::catalog::validate_identifier(&delivery.architecture)?;
            if delivery.architecture != ARCHITECTURE
                || !ids.insert(delivery.id.as_str())
                || delivery.files.is_empty()
                || delivery.files.len() > MAX_MODEL_FILES
            {
                bail!("model delivery identity is invalid");
            }
            let component = component_catalog
                .components
                .iter()
                .find(|component| component.id == delivery.id)
                .ok_or_else(|| anyhow!("model delivery is missing from the component catalog"))?;
            if !matches!(component.kind, super::catalog::ComponentKind::Model)
                || !component.removable
                || !component.dependencies.is_empty()
            {
                bail!("model component catalog boundary is invalid");
            }
            validate_files(delivery)?;
            validate_legacy(delivery.legacy_root.as_ref())?;
        }
        for kind in [
            ModelKind::QwenSmall,
            ModelKind::QwenLarge,
            ModelKind::StepAudio,
            ModelKind::Magpie,
            ModelKind::Kokoro,
            ModelKind::Supertonic,
            ModelKind::Vieneu,
        ] {
            if !ids.contains(kind.id()) {
                bail!("model delivery catalog is incomplete");
            }
        }
        Ok(())
    }
}

fn validate_files(delivery: &ModelDelivery) -> Result<()> {
    let archive_delivery = delivery.archive.is_some();
    let mut paths = HashSet::new();
    let mut total = 0_u64;
    for file in &delivery.files {
        super::receipt::validate_relative_path(&file.path)?;
        if !paths.insert(file.path.clone()) || file.size_bytes == 0 {
            bail!("model delivery contains an invalid file");
        }
        validate_hash(&file.sha256)?;
        match (&file.url, archive_delivery) {
            (None, true) => {}
            (Some(url), false) => validate_url(url, None)?,
            _ => bail!("model delivery mixes archive and direct file sources"),
        }
        total = total
            .checked_add(file.size_bytes)
            .ok_or_else(|| anyhow!("model delivery size overflow"))?;
    }
    if total != delivery.installed_size_bytes || total > MAX_MODEL_BYTES {
        bail!("model delivery installed size is invalid");
    }
    if let Some(archive) = &delivery.archive {
        if archive.size_bytes == 0 {
            bail!("model archive size is invalid");
        }
        validate_hash(&archive.sha256)?;
        validate_url(&archive.url, Some(&archive.sha256))?;
    }
    Ok(())
}

fn validate_legacy(legacy: Option<&LegacyRoot>) -> Result<()> {
    let Some(legacy) = legacy else {
        return Ok(());
    };
    super::catalog::validate_identifier(&legacy.path)?;
    if legacy.path.contains(['/', '\\']) {
        bail!("model legacy root is unsafe");
    }
    Ok(())
}

fn validate_url(url: &str, content_hash: Option<&str>) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("https://")
        || lower.contains("/resolve/main/")
        || lower.contains("/resolve/master/")
        || lower.contains("/latest/")
        || lower.contains("nightly")
        || lower.contains('?')
        || content_hash.is_some_and(|hash| !lower.contains(&hash[..16]))
    {
        bail!("model delivery URL is mutable or not content-addressed");
    }
    Ok(())
}

fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("model delivery checksum is invalid");
    }
    Ok(())
}

fn version_root(delivery: &ModelDelivery) -> Result<PathBuf> {
    super::component_version_root(&delivery.id, &delivery.version)
}

fn validate_status(delivery: &ModelDelivery) -> Result<()> {
    let root = version_root(delivery)?;
    let stamp = status_stamp(&root)?;
    if let Some(valid) = cached_status(delivery, &stamp) {
        return if valid {
            Ok(())
        } else {
            Err(anyhow!("installed model status does not match this build"))
        };
    }
    let result = validate_status_uncached(delivery, &root);
    STATUS_CACHE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .insert(
            delivery.id.clone(),
            CachedStatus {
                version: delivery.version.clone(),
                stamp,
                valid: result.is_ok(),
            },
        );
    result
}

fn status_stamp(root: &Path) -> Result<StatusStamp> {
    let root_metadata = std::fs::symlink_metadata(root)?;
    if !root_metadata.is_dir() || is_reparse_point(&root_metadata) {
        bail!("model component root is unsafe");
    }
    let receipt_metadata = std::fs::symlink_metadata(root.join(RECEIPT_NAME))?;
    if !receipt_metadata.is_file() || is_reparse_point(&receipt_metadata) {
        bail!("model component receipt is unsafe");
    }
    Ok(StatusStamp {
        root_modified: root_metadata.modified().ok(),
        receipt_bytes: receipt_metadata.len(),
        receipt_modified: receipt_metadata.modified().ok(),
    })
}

fn cached_status(delivery: &ModelDelivery, stamp: &StatusStamp) -> Option<bool> {
    let cache = STATUS_CACHE
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let cached = cache.get(&delivery.id)?;
    (cached.version == delivery.version && &cached.stamp == stamp).then_some(cached.valid)
}

fn cache_invalid_status(delivery: &ModelDelivery, root: &Path) {
    let Ok(stamp) = status_stamp(root) else {
        invalidate_status(&delivery.id);
        return;
    };
    STATUS_CACHE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .insert(
            delivery.id.clone(),
            CachedStatus {
                version: delivery.version.clone(),
                stamp,
                valid: false,
            },
        );
}

fn validate_status_uncached(delivery: &ModelDelivery, root: &Path) -> Result<()> {
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != delivery.id
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || !receipt.dependencies.is_empty()
        || receipt.files.len() != delivery.files.len()
    {
        bail!("model receipt does not match this build");
    }
    for (owned, expected) in receipt.files.iter().zip(&delivery.files) {
        if owned.path != expected.path
            || owned.size_bytes != expected.size_bytes
            || !owned.sha256.eq_ignore_ascii_case(&expected.sha256)
            || !size_matches(&resolve_owned_path(root, &owned.path)?, expected)
        {
            bail!("installed model size inventory does not match this build");
        }
    }
    validate_exact_tree(root, &delivery.files)
}

pub(crate) fn invalidate_status(id: &str) {
    STATUS_CACHE
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .remove(id);
}

fn size_matches(path: &Path, expected: &ModelFile) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && !is_reparse_point(&metadata) && metadata.len() == expected.size_bytes
    })
}

fn validate_exact_tree(root: &Path, files: &[ModelFile]) -> Result<()> {
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_MODEL_FILES + 1)?;
    actual.sort();
    let mut expected = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    expected.push(PathBuf::from(RECEIPT_NAME));
    expected.sort();
    if actual != expected {
        bail!("model component contains unowned files");
    }
    Ok(())
}

fn lock_and_verify_files(root: &Path, files: &[ModelFile]) -> Result<Vec<std::fs::File>> {
    #[cfg(test)]
    MODEL_HASH_PASSES.with(|passes| passes.set(passes.get() + 1));
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, &expected.path)?;
        let mut file = open_locked_regular_file(&path)?;
        let metadata = file.metadata()?;
        if metadata.len() != expected.size_bytes {
            bail!("model changed while acquiring its use guard");
        }
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0_u8; 256 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            use sha2::Digest as _;
            hasher.update(&buffer[..read]);
        }
        use sha2::Digest as _;
        if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected.sha256) {
            bail!("model changed while acquiring its use guard");
        }
        locked.push(file);
    }
    Ok(locked)
}

#[cfg(test)]
thread_local! {
    static MODEL_HASH_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static POST_HASH_TEST_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(all(test, not(feature = "recorder-worker")))]
fn reset_model_hash_passes() {
    MODEL_HASH_PASSES.with(|passes| passes.set(0));
}

#[cfg(all(test, not(feature = "recorder-worker")))]
fn model_hash_passes() -> usize {
    MODEL_HASH_PASSES.with(std::cell::Cell::get)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
fn set_post_hash_test_hook(hook: impl FnOnce() + 'static) {
    POST_HASH_TEST_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_post_hash_test_hook() {
    let hook = POST_HASH_TEST_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("model load file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
        options.share_mode(FILE_SHARE_READ.0);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("model load file is unsafe");
    }
    Ok(file)
}

fn owned_file(file: &ModelFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: file.path.clone(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.clone(),
    }
}

fn legacy_root(delivery: &ModelDelivery) -> Option<PathBuf> {
    let legacy = delivery.legacy_root.as_ref()?;
    let root = match legacy.kind {
        LegacyRootKind::RoamingModels => crate::paths::app_models_dir(),
        LegacyRootKind::LocalModels => crate::paths::app_local_data_dir().join("models"),
    };
    Some(root.join(&legacy.path))
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;
