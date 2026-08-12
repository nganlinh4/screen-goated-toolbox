//! Verified delivery for optional WebView frontend bundles.

use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, file_size_matches,
    is_reparse_point, resolve_owned_path, validate_relative_path,
};
use super::{ComponentLease, RemovalOutcome};

mod notifications;
mod staging;
mod update;
mod validation;

use validation::{validate_exact_tree, validate_install, validate_status};

const MAX_ARCHIVE_ENTRIES: usize = 64;
const ARCHITECTURE: &str = "x64";
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WebAssetComponent {
    Creation3d,
    PromptDj,
    TtsPlayground,
}

impl WebAssetComponent {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Creation3d => "creation-3d-web",
            Self::PromptDj => "prompt-dj-web",
            Self::TtsPlayground => "tts-playground-web",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Creation3d => "3D Creation interface",
            Self::PromptDj => "PromptDJ interface",
            Self::TtsPlayground => "TTS Playground interface",
        }
    }

    const fn dependencies(self) -> &'static [&'static str] {
        match self {
            Self::Creation3d | Self::PromptDj | Self::TtsPlayground => &[],
        }
    }
}

struct WebAssetFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct WebAssetDelivery {
    component: WebAssetComponent,
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [WebAssetFile],
}

include!(concat!(env!("OUT_DIR"), "/web_asset_delivery.rs"));

pub(crate) struct WebAssetPack {
    root: PathBuf,
    delivery: &'static WebAssetDelivery,
    _lease: ComponentLease,
}

impl WebAssetPack {
    pub(crate) fn read(&self, relative: &str) -> Result<Vec<u8>> {
        let relative = Path::new(relative);
        validate_relative_path(relative)?;
        let expected = self
            .delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("web asset is not owned by this package"))?;
        let owned = owned_file(expected);
        let path = resolve_owned_path(&self.root, relative)?;
        if !file_matches(&path, &owned)? {
            bail!("installed web asset failed integrity verification");
        }
        read_regular_file(&path)
    }
}

pub(crate) fn open(component: WebAssetComponent) -> Result<WebAssetPack> {
    let delivery = delivery(component).ok_or_else(|| {
        anyhow!(
            "{} download contract is unavailable",
            component.display_name()
        )
    })?;
    let lease = super::acquire(component.id())?;
    validate_install(delivery)?;
    Ok(WebAssetPack {
        root: version_root(delivery)?,
        delivery,
        _lease: lease,
    })
}

pub(crate) fn launch_when_ready(component: WebAssetComponent, launch: fn()) {
    let update_due = super::update_catalog::refresh_due(component.id(), "before-open");
    if is_installed(component) && !update_due {
        launch();
        return;
    }
    static INSTALLING: LazyLock<Mutex<HashSet<&'static str>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));
    {
        let mut installing = INSTALLING.lock().unwrap_or_else(|value| value.into_inner());
        if !installing.insert(component.id()) {
            return;
        }
    }
    std::thread::spawn(move || {
        super::update_catalog::refresh_for_use(component.id(), "before-open");
        let result = download(component, Arc::new(AtomicBool::new(false)), true);
        INSTALLING
            .lock()
            .unwrap_or_else(|value| value.into_inner())
            .remove(component.id());
        match result {
            Ok(()) => launch(),
            Err(error) => {
                crate::log_info!("[Web assets] {}: {error}", component.id());
                notify_install_error(component, &error);
            }
        }
    });
}

pub(crate) fn is_installed(component: WebAssetComponent) -> bool {
    delivery(component).is_some_and(|delivery| validate_install(delivery).is_ok())
}

pub(crate) fn is_installed_for_display(component: WebAssetComponent) -> bool {
    delivery(component).is_some_and(|delivery| validate_status(delivery).is_ok())
}

pub(crate) fn download_title(component: WebAssetComponent) -> String {
    notifications::download_title(component)
}

pub(crate) fn component_dir(component: WebAssetComponent) -> PathBuf {
    delivery(component)
        .and_then(|delivery| version_root(delivery).ok())
        .unwrap_or_else(|| super::components_root().join(component.id()))
}

pub(crate) fn remove(component: WebAssetComponent) -> Result<()> {
    match super::request_remove(component.id())? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{} contains {} modified managed file(s); they were preserved",
            component.display_name(),
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{} is required by installed components: {}",
            component.display_name(),
            dependents.join(", ")
        ),
    }
}

pub(crate) fn download(
    component: WebAssetComponent,
    stop: Arc<AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let _mutation = super::acquire_mutation_guard()?;
    static DOWNLOAD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    let _guard = DOWNLOAD_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if is_installed(component) {
        return Ok(());
    }
    let delivery = delivery(component).ok_or_else(|| {
        anyhow!(
            "{} download contract is unavailable",
            component.display_name()
        )
    })?;
    clear_invalid_install(component)?;
    let _install_lease = super::acquire(component.id())?;
    let badge = use_badge.then(|| {
        crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&notifications::localized_name(
            component,
        ))
    });
    notifications::set_download_state(component, 0.0);
    let result = install_delivery(delivery, &stop, |downloaded, total| {
        let progress = downloaded as f32 / total as f32 * 100.0;
        notifications::set_download_state(component, progress);
        if let Some(badge) = &badge {
            badge.report(downloaded, total);
        }
    });
    notifications::finish_download_state(result.is_ok());
    if let Some(badge) = &badge {
        badge.finish();
    }
    if use_badge && result.is_ok() {
        notifications::notify_success(component);
    }
    result
}

pub(crate) fn download_from_manager(
    component: WebAssetComponent,
    stop: Arc<AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let result = download(component, stop, use_badge);
    if let Err(error) = &result {
        crate::log_info!("[Web assets] {}: {error}", component.id());
        notify_install_error(component, error);
    }
    result
}

fn notify_install_error(component: WebAssetComponent, error: &anyhow::Error) {
    let name = notifications::localized_name(component);
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        locale.component_install_failed_fmt,
        &[("name", &name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &title,
        &error.to_string(),
        crate::overlay::auto_copy_badge::NotificationType::Error,
    );
}

fn delivery(component: WebAssetComponent) -> Option<&'static WebAssetDelivery> {
    if let Some(delivery) = update::deliveries()
        .iter()
        .find(|delivery| delivery.component == component)
    {
        return Some(delivery);
    }
    WEB_ASSET_DELIVERIES
        .iter()
        .find(|delivery| delivery.component == component)
}

fn version_root(delivery: &WebAssetDelivery) -> Result<PathBuf> {
    super::component_version_root(delivery.component.id(), delivery.version)
}

fn clear_invalid_install(component: WebAssetComponent) -> Result<()> {
    match super::request_remove(component.id())? {
        RemovalOutcome::Missing | RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::Pending => bail!("{} is currently in use", component.display_name()),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{} cannot be repaired because {} modified managed file(s) were preserved",
            component.display_name(),
            paths.len()
        ),
        RemovalOutcome::RequiredBy(dependents) => bail!(
            "{} cannot be repaired while required by installed components: {}",
            component.display_name(),
            dependents.join(", ")
        ),
    }
}

fn install_delivery(
    delivery: &WebAssetDelivery,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let scratch = crate::paths::app_local_data_dir().join("component-downloads");
    std::fs::create_dir_all(&scratch)?;
    require_regular_directory(&scratch)?;
    let archive_path = scratch.join(format!(
        "{}-{}-{sequence}.download",
        delivery.component.id(),
        std::process::id()
    ));
    let staging_parent = crate::paths::app_local_data_dir().join("component-staging");
    std::fs::create_dir_all(&staging_parent)?;
    require_regular_directory(&staging_parent)?;
    let staging = staging_parent.join(format!(
        "{}-{}-{}-{sequence}",
        delivery.component.id(),
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;

    let result = (|| {
        download_archive(delivery, &archive_path, stop, on_progress)?;
        validate_archive(&archive_path, delivery)?;
        extract_archive(&archive_path, &staging, delivery)?;
        super::write_receipt(&staging, &receipt(delivery))?;
        validate_staging(&staging, delivery)?;
        let target = version_root(delivery)?;
        let parent = super::ensure_component_parent(delivery.component.id())?;
        if target.parent() != Some(parent.as_path()) {
            bail!("web asset install target escaped its component directory");
        }
        if target.exists() {
            bail!("web asset install target already exists");
        }
        std::fs::rename(&staging, &target)?;
        super::validate_version_root(delivery.component.id(), delivery.version)?;
        validate_install(delivery)
    })();
    let _ = std::fs::remove_file(&archive_path);
    if staging.exists() {
        let owned_paths = delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        let _ = staging::cleanup_owned(&staging, &owned_paths);
    }
    result
}

fn download_archive(
    delivery: &WebAssetDelivery,
    target: &Path,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| {
            anyhow!(
                "{} download failed: {error}",
                delivery.component.display_name()
            )
        })?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("web asset download size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    loop {
        if stop.load(Ordering::Relaxed) {
            bail!("web asset download was cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("web asset download is too large"))?;
        if downloaded > delivery.size_bytes {
            bail!("web asset download is larger than this build allows");
        }
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, delivery.size_bytes);
    }
    if downloaded != delivery.size_bytes {
        bail!("web asset download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn validate_archive(path: &Path, delivery: &WebAssetDelivery) -> Result<()> {
    let owned = OwnedComponentFile {
        path: PathBuf::from(delivery.asset),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &owned)? {
        bail!("web asset archive checksum mismatch");
    }
    Ok(())
}

fn extract_archive(path: &Path, staging: &Path, delivery: &WebAssetDelivery) -> Result<()> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!("web asset archive has an unexpected entry count");
    }
    let mut extracted = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("web asset archive contains an unexpected directory entry");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("web asset archive contains an unsafe path"))?
            .to_path_buf();
        validate_relative_path(&relative)?;
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("web asset archive contains an unowned file"))?;
        if !extracted.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("web asset archive entry does not match its manifest");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("web asset archive expands beyond its limit"))?;
        if extracted_bytes > delivery.unpacked_size_bytes {
            bail!("web asset archive expands beyond its declared size");
        }
        let target = staging::prepare_target(staging, &relative)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        let owned = owned_file(expected);
        if !file_matches(&target, &owned)? {
            bail!("extracted web asset checksum mismatch");
        }
    }
    if extracted.len() != delivery.files.len() || extracted_bytes != delivery.unpacked_size_bytes {
        bail!("web asset archive is incomplete");
    }
    Ok(())
}

fn validate_staging(staging: &Path, delivery: &WebAssetDelivery) -> Result<()> {
    for expected in delivery.files {
        let owned = owned_file(expected);
        if !file_matches(&staging.join(&owned.path), &owned)? {
            bail!("staged web asset failed integrity verification");
        }
    }
    validate_exact_tree(staging, delivery)
}

fn receipt(delivery: &WebAssetDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: delivery.component.id().to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: delivery
            .component
            .dependencies()
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

fn owned_file(file: &WebAssetFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: PathBuf::from(file.path),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("web asset is not a regular file");
    }
    Ok(std::fs::read(path)?)
}

fn require_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("web asset working directory is unsafe");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
