//! Verified delivery for optional WebView frontend bundles.

use crate::component_registry::verified_archive::{self, FileSpec, Policy};
use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context as _, Result, anyhow, bail};

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, file_matches, file_size_matches,
    is_reparse_point, read_verified_file, resolve_owned_path, validate_relative_path,
};
use super::{ComponentLease, RemovalOutcome, persisted_download};

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
    PromptDj,
    TtsPlayground,
}

impl WebAssetComponent {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::PromptDj => "prompt-dj-web",
            Self::TtsPlayground => "tts-playground-web",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::PromptDj => "PromptDJ interface",
            Self::TtsPlayground => "TTS Playground interface",
        }
    }

    const fn dependencies(self) -> &'static [&'static str] {
        match self {
            Self::PromptDj | Self::TtsPlayground => &[],
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
        read_verified_file(&path, &owned)
            .context("installed web asset failed integrity verification")
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
    crate::task_runtime::spawn_detached(
        crate::task_runtime::TaskClass::Io,
        "web-asset-install",
        move || {
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
        },
    );
}

pub(crate) fn is_installed(component: WebAssetComponent) -> bool {
    delivery(component).is_some_and(|delivery| validate_install(delivery).is_ok())
}

pub(crate) fn is_installed_for_display(component: WebAssetComponent) -> bool {
    delivery(component).is_some_and(|delivery| validate_status(delivery).is_ok())
}

pub(crate) fn component_dir(component: WebAssetComponent) -> PathBuf {
    delivery(component)
        .and_then(|delivery| version_root(delivery).ok())
        .unwrap_or_else(|| super::components_root().join(component.id()))
}

pub(crate) fn remove(component: WebAssetComponent) -> Result<()> {
    match super::request_remove_and_wait(component.id())? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::PreservedModified(paths) => bail!(
            "{} contains {} unrecorded or unsafe path(s); they were preserved",
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
    let _activity = crate::install_activity::register(stop.clone())?;
    let progress_guard = crate::install_progress::begin(component.id());
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
    let badge = use_badge.then(|| {
        crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&notifications::localized_name(
            component,
        ))
    });
    notifications::set_download_state(component, 0.0);
    let report = |downloaded: u64, total: u64| {
        let progress = downloaded as f32 / total as f32 * 100.0;
        progress_guard.report(progress);
        notifications::set_download_state(component, progress);
        if let Some(badge) = &badge {
            badge.report(downloaded, total);
        }
    };
    // The transfer holds only this component's transfer guard; the registry
    // guard covers extraction and publication.
    let result = prefetch(delivery, &stop, &report).and_then(|()| {
        let _mutation = super::acquire_mutation_guard()?;
        if is_installed(component) {
            return Ok(());
        }
        let mut install_lease = super::acquire(component.id())?;
        install_delivery(delivery, &stop, report, &mut install_lease)
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

fn install_delivery(
    delivery: &WebAssetDelivery,
    stop: &AtomicBool,
    on_progress: impl Fn(u64, u64),
    install_lease: &mut ComponentLease,
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging_parent = crate::paths::app_runtime_local_data_dir().join("component-staging");
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
        let archive_path = persisted_download::fetch(
            delivery.component.id(),
            &pinned_body(delivery),
            stop,
            &on_progress,
        )
        .with_context(|| format!("{} download failed", delivery.component.display_name()))?;
        validate_archive(&archive_path, delivery)?;
        extract_archive(&archive_path, &staging, delivery, stop)?;
        super::write_receipt(&staging, &receipt(delivery))?;
        validate_staging(&staging, delivery)?;
        if stop.load(Ordering::Acquire) {
            bail!("web asset installation was cancelled");
        }
        super::publication::publish_verified(
            delivery.component.id(),
            delivery.version,
            &receipt(delivery).files,
            &staging,
            install_lease,
        )?;
        super::validate_version_root(delivery.component.id(), delivery.version)?;
        validate_install(delivery)?;
        // A failed install keeps the received archive for the next attempt.
        persisted_download::discard_leftovers(delivery.component.id());
        Ok(())
    })();
    if staging.exists() {
        let mut owned_paths = delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        owned_paths.push(RECEIPT_NAME);
        let _ = staging::cleanup_owned(&staging, &owned_paths);
    }
    result
}

/// Transfers the archive of a fresh install while holding only this
/// component's transfer guard, before the caller takes the registry guard.
fn prefetch(
    delivery: &WebAssetDelivery,
    stop: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    persisted_download::prefetch(
        delivery.component.id(),
        &[pinned_body(delivery)],
        stop,
        on_progress,
    )
    .with_context(|| format!("{} download failed", delivery.component.display_name()))
}

fn pinned_body(delivery: &WebAssetDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
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

fn extract_archive(
    path: &Path,
    staging: &Path,
    delivery: &WebAssetDelivery,
    stop: &AtomicBool,
) -> Result<()> {
    let files = delivery
        .files
        .iter()
        .map(|file| FileSpec::same(Path::new(file.path), file.size_bytes, file.sha256))
        .collect::<Vec<_>>();
    verified_archive::extract(
        std::fs::File::open(path)?,
        &files,
        Policy::files(MAX_ARCHIVE_ENTRIES, delivery.unpacked_size_bytes),
        stop,
        |index, member| {
            let expected = &delivery.files[index];
            let (target, _directories) =
                staging::prepare_target(staging, Path::new(expected.path))?;
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)?;
            member.copy_to(&mut output, &|_| {})?;
            output.flush()?;
            drop(output);
            if !file_matches(&target, &owned_file(expected))? {
                bail!("extracted component file failed integrity verification");
            }

            Ok(())
        },
    )
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

fn require_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("web asset working directory is unsafe");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
