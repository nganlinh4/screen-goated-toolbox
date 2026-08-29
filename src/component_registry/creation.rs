//! One verified Windows archive for the complete Creation product.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::receipt::{
    OwnedComponentFile, file_matches, resolve_owned_path, validate_relative_path,
};
use super::{ComponentLease, RemovalOutcome};

mod install;
mod staging;
mod update;
mod validation;

const COMPONENT_ID: &str = "creation-windows";
const ARCHITECTURE: &str = "x64";
const MAX_ARCHIVE_ENTRIES: usize = 16;
const RUNTIME_PATH: &str = "bin/sgt_creation_runtime.exe";

struct CreationFile {
    path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct CreationDelivery {
    version: &'static str,
    runtime_version: &'static str,
    features: &'static [&'static str],
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [CreationFile],
}

include!(concat!(env!("OUT_DIR"), "/creation_delivery.rs"));

pub(crate) struct CreationPack {
    root: PathBuf,
    delivery: &'static CreationDelivery,
    _lease: ComponentLease,
}

impl CreationPack {
    pub(crate) fn read_web(&self, relative: &str) -> Result<Vec<u8>> {
        let relative = Path::new(relative);
        validate_relative_path(relative)?;
        let path = Path::new("web").join(relative);
        let expected = expected_file(self.delivery, &path)?;
        let target = resolve_owned_path(&self.root, &path)?;
        if !file_matches(&target, &owned_file(expected))? {
            bail!("installed Creation interface failed integrity verification");
        }
        read_regular_file(&target)
    }
}

pub(crate) struct RuntimeUse {
    path: PathBuf,
    _file: std::fs::File,
    _lease: ComponentLease,
}

impl RuntimeUse {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn open() -> Result<CreationPack> {
    let delivery =
        delivery().ok_or_else(|| anyhow!("Creation download contract is unavailable"))?;
    let lease = super::acquire(COMPONENT_ID)?;
    validation::validate_install(delivery)?;
    Ok(CreationPack {
        root: version_root(delivery)?,
        delivery,
        _lease: lease,
    })
}

pub(crate) fn open_runtime() -> Result<RuntimeUse> {
    let delivery =
        delivery().ok_or_else(|| anyhow!("Creation download contract is unavailable"))?;
    let lease = super::acquire(COMPONENT_ID)?;
    validation::validate_install(delivery)?;
    let root = version_root(delivery)?;
    let relative = Path::new(RUNTIME_PATH);
    let expected = expected_file(delivery, relative)?;
    let path = resolve_owned_path(&root, relative)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.share_mode(0x0000_0001);
    }
    let file = options.open(&path)?;
    if !file_matches(&path, &owned_file(expected))? {
        bail!("installed Creation engine failed integrity verification");
    }
    Ok(RuntimeUse {
        path,
        _file: file,
        _lease: lease,
    })
}

pub(crate) fn launch_when_ready(launch: fn()) {
    let update_due = super::update_catalog::refresh_due(COMPONENT_ID, "before-open");
    if is_installed() && !update_due {
        launch();
        return;
    }
    static INSTALLING: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));
    {
        let mut installing = INSTALLING.lock().unwrap_or_else(|value| value.into_inner());
        if *installing {
            return;
        }
        *installing = true;
    }
    crate::task_runtime::spawn_detached(
        crate::task_runtime::TaskClass::Io,
        "creation-install",
        move || {
            super::update_catalog::refresh_for_use(COMPONENT_ID, "before-open");
            let result = download(Arc::new(AtomicBool::new(false)), true);
            *INSTALLING.lock().unwrap_or_else(|value| value.into_inner()) = false;
            match result {
                Ok(()) => launch(),
                Err(error) => notify_error(&error),
            }
        },
    );
}

pub(crate) fn download(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let _activity = crate::install_activity::register(stop.clone())?;
    install::download(stop, use_badge)
}

pub(crate) fn is_installed() -> bool {
    delivery().is_some_and(|delivery| validation::validate_install(delivery).is_ok())
}

pub(crate) fn is_installed_for_display() -> bool {
    delivery().is_some_and(|delivery| validation::validate_status(delivery).is_ok())
}

pub(crate) fn is_available() -> bool {
    delivery().is_some()
}

pub(crate) fn is_partially_installed() -> bool {
    [COMPONENT_ID, "creation-3d-web", "creation-3d-runtime"]
        .into_iter()
        .any(|id| super::components_root().join(id).exists())
}

pub(crate) fn component_dir() -> PathBuf {
    delivery()
        .and_then(|delivery| version_root(delivery).ok())
        .unwrap_or_else(|| super::components_root().join(COMPONENT_ID))
}

pub(crate) fn runtime_version() -> &'static str {
    delivery().map_or("not-included", |delivery| delivery.runtime_version)
}

pub(crate) fn supports(feature: &str) -> bool {
    delivery().is_some_and(|delivery| delivery.features.contains(&feature))
}

pub(crate) fn refresh_after_start_failure() -> bool {
    let Some((mode, _, _)) = super::update_catalog::policy(COMPONENT_ID) else {
        return false;
    };
    if mode != "before-open" {
        return false;
    }
    let before = delivery().map(|delivery| (delivery.version, delivery.sha256));
    if super::update_catalog::refresh_now().is_err() {
        return false;
    }
    let after = delivery().map(|delivery| (delivery.version, delivery.sha256));
    before != after && after.is_some() && download(Arc::new(AtomicBool::new(false)), true).is_ok()
}

pub(crate) fn remove() -> Result<()> {
    match super::request_remove_and_wait(COMPONENT_ID)? {
        RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => Ok(()),
        RemovalOutcome::RequiredBy(dependents) => {
            bail!("Creation is required by: {}", dependents.join(", "))
        }
        RemovalOutcome::PreservedModified(paths) => bail!(
            "Creation contains {} unsafe or unrecorded path(s); they were preserved",
            paths.len()
        ),
    }
}

pub(crate) fn remove_legacy_components() -> Result<()> {
    for id in ["creation-3d-web", "creation-3d-runtime"] {
        match super::request_remove_and_wait(id)? {
            RemovalOutcome::Missing | RemovalOutcome::Removed | RemovalOutcome::Pending => {}
            RemovalOutcome::RequiredBy(dependents) => bail!(
                "Legacy Creation payload is required by: {}",
                dependents.join(", ")
            ),
            RemovalOutcome::PreservedModified(paths) => bail!(
                "Legacy Creation payload preserved {} unsafe path(s)",
                paths.len()
            ),
        }
    }
    Ok(())
}

fn delivery() -> Option<&'static CreationDelivery> {
    update::delivery().or(CREATION_DELIVERY.as_ref())
}

fn version_root(delivery: &CreationDelivery) -> Result<PathBuf> {
    super::component_version_root(COMPONENT_ID, delivery.version)
}

fn expected_file<'a>(delivery: &'a CreationDelivery, path: &Path) -> Result<&'a CreationFile> {
    delivery
        .files
        .iter()
        .find(|file| Path::new(file.path) == path)
        .ok_or_else(|| anyhow!("Creation file is not owned by its delivery contract"))
}

fn owned_file(file: &CreationFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: file.path.into(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || super::receipt::is_reparse_point(&metadata) {
        bail!("Creation asset is not a regular file");
    }
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn localized_name() -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    crate::gui::locale::LocaleText::get(&language)
        .auxiliary
        .managed_tools
        .tool_creation_card
        .to_string()
}

fn notify_error(error: &anyhow::Error) {
    let name = localized_name();
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

#[cfg(test)]
mod tests;
