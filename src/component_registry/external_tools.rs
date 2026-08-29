//! Exact, removable delivery and process-lifetime guards for Windows external tools.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use anyhow::{Context as _, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use super::receipt::{
    ComponentReceipt, OwnedComponentFile, RECEIPT_NAME, is_reparse_point, resolve_owned_path,
};
use super::{ComponentLease, RemovalOutcome};

mod install;
mod progress;
mod recovery;
mod recovery_io;
mod staging;
mod update;

pub(crate) use progress::{
    ExternalToolInstallEvent, localized_install_event_message, report_badge_event,
};
pub(crate) use recovery::{ExternalToolRecovery, RecoveryCleanupOutcome};

const ARCHITECTURE: &str = "x64";
const MAX_COMPONENT_FILES: usize = 8;
static TOOL_MUTATION_LOCKS: LazyLock<[Mutex<()>; 3]> =
    LazyLock::new(|| std::array::from_fn(|_| Mutex::new(())));
static PERIODIC_UPDATE_IDENTITY: LazyLock<Mutex<Option<(String, String)>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalTool {
    YtDlp,
    Ffmpeg,
    Deno,
}

impl ExternalTool {
    pub(crate) const ALL: [Self; 3] = [Self::YtDlp, Self::Ffmpeg, Self::Deno];

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp-x64",
            Self::Ffmpeg => "ffmpeg-x64",
            Self::Deno => "deno-x64",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.id() == id)
    }

    const fn executable(self) -> &'static str {
        match self {
            Self::YtDlp => "bin/x64/yt-dlp.exe",
            Self::Ffmpeg => "bin/x64/ffmpeg.exe",
            Self::Deno => "bin/x64/deno.exe",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ExternalArchiveFormat {
    Raw,
    Zip,
}

const SUPPORTED_ARCHIVE_FORMATS: [ExternalArchiveFormat; 2] =
    [ExternalArchiveFormat::Raw, ExternalArchiveFormat::Zip];

#[derive(Clone, Copy)]
struct ExternalToolFile {
    path: &'static str,
    archive_path: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

struct ExternalToolDelivery {
    id: &'static str,
    version: &'static str,
    asset: &'static str,
    download_url: &'static str,
    archive_format: ExternalArchiveFormat,
    size_bytes: u64,
    sha256: &'static str,
    unpacked_size_bytes: u64,
    files: &'static [ExternalToolFile],
}

pub(crate) struct WebView2BootstrapperDelivery {
    pub(crate) version: &'static str,
    pub(crate) asset: &'static str,
    pub(crate) download_url: &'static str,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: &'static str,
    pub(crate) expected_publisher: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/external_tool_delivery.rs"));

#[derive(Clone, Debug)]
pub(crate) enum ExternalToolStatus {
    Installed { bytes: u64 },
    Missing,
    Unavailable,
    Error(String),
}

pub(crate) struct ExternalToolUse {
    tool: ExternalTool,
    root: PathBuf,
    _files: Vec<std::fs::File>,
    _lease: ComponentLease,
}

impl ExternalToolUse {
    pub(crate) fn tool(&self) -> ExternalTool {
        self.tool
    }

    pub(crate) fn executable(&self) -> PathBuf {
        self.root.join(self.tool.executable())
    }

    pub(crate) fn bin_dir(&self) -> PathBuf {
        self.root.join("bin/x64")
    }
}

pub(crate) fn ensure(
    tool: ExternalTool,
    cancelled: &AtomicBool,
    on_event: impl Fn(ExternalToolInstallEvent),
) -> Result<ExternalToolUse> {
    let delivery = match delivery(tool) {
        Ok(delivery) => delivery,
        Err(error) => {
            crate::log_info!(
                "[ExternalTools] ensure_failed component={} stage=resolve_contract error={error:#}",
                tool.id()
            );
            return Err(error);
        }
    };
    let target = version_root(delivery)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|error| format!("<unresolved: {error:#}>"));
    crate::log_info!(
        "[ExternalTools] ensure_start component={} version={} format={:?} target={target}",
        delivery.id,
        delivery.version,
        delivery.archive_format
    );
    on_event(ExternalToolInstallEvent::Preparing);
    // External operations intentionally coalesce in-process callers before
    // taking the cross-process registry mutex. Generic removal never takes
    // this local lock, so the local -> global order cannot form a cycle.
    let _local = lock_tool_mutation(tool);
    let result = (|| {
        let _global = super::acquire_mutation_guard()
            .with_context(|| format!("acquire registry mutation guard for {}", delivery.id))?;
        install::ensure(tool, delivery, cancelled, on_event)
    })();
    match &result {
        Ok(_) => crate::log_info!(
            "[ExternalTools] ensure_ready component={} version={} target={target}",
            delivery.id,
            delivery.version
        ),
        Err(error) => crate::log_info!(
            "[ExternalTools] ensure_failed component={} version={} target={target} error={error:#}",
            delivery.id,
            delivery.version
        ),
    }
    result
}

pub(crate) fn acquire_installed(tool: ExternalTool) -> Result<ExternalToolUse> {
    acquire_delivery(tool, delivery(tool)?)
}

pub(crate) fn expected_executable_path(tool: ExternalTool) -> Result<PathBuf> {
    Ok(version_root(delivery(tool)?)?.join(tool.executable()))
}

pub(crate) fn current_status(tool: ExternalTool) -> ExternalToolStatus {
    let Some(delivery) = delivery_optional(tool) else {
        return ExternalToolStatus::Unavailable;
    };
    match validate_install_fast(delivery) {
        Ok(()) => ExternalToolStatus::Installed {
            bytes: delivery.unpacked_size_bytes,
        },
        Err(error) if version_root(delivery).is_ok_and(|root| root.exists()) => {
            ExternalToolStatus::Error(error.to_string())
        }
        Err(_) => ExternalToolStatus::Missing,
    }
}

pub(crate) fn version_label(tool: ExternalTool) -> Option<String> {
    delivery_optional(tool).map(|delivery| format!("{} (x64)", delivery.version))
}

pub(crate) fn refresh_downloader_after_failure(include_deno: bool) -> Result<Vec<ExternalTool>> {
    let candidates = [ExternalTool::YtDlp, ExternalTool::Deno]
        .into_iter()
        .filter(|tool| *tool != ExternalTool::Deno || include_deno)
        .filter(|tool| {
            super::update_catalog::policy(tool.id())
                .is_some_and(|(mode, _, _)| mode == "typed-failure")
        })
        .collect::<Vec<_>>();
    let before = candidates
        .iter()
        .map(|tool| {
            (
                *tool,
                delivery_optional(*tool).map(|delivery| (delivery.version, delivery.sha256)),
            )
        })
        .collect::<Vec<_>>();
    super::update_catalog::refresh_now()?;
    Ok(before
        .into_iter()
        .filter_map(|(tool, previous)| {
            let current =
                delivery_optional(tool).map(|delivery| (delivery.version, delivery.sha256));
            (previous.is_some() && current.is_some() && previous != current).then_some(tool)
        })
        .collect())
}

pub(crate) fn schedule_periodic_updates() {
    let Some((mode, _, _)) = super::update_catalog::policy(ExternalTool::Ffmpeg.id()) else {
        return;
    };
    if mode != "periodic-idle" {
        return;
    }
    let Some(delivery) = delivery_optional(ExternalTool::Ffmpeg) else {
        return;
    };
    let identity = (delivery.version.to_string(), delivery.sha256.to_string());
    let should_check = PERIODIC_UPDATE_IDENTITY
        .lock()
        .map(|mut checked| {
            if checked.as_ref() == Some(&identity) {
                false
            } else {
                *checked = Some(identity);
                true
            }
        })
        .unwrap_or(false);
    if !should_check {
        return;
    }
    if let Err(error) = crate::task_runtime::spawn_with_timeout(
        crate::task_runtime::TaskClass::Maintenance,
        "external-tool-periodic-update",
        std::time::Duration::from_secs(10 * 60),
        |_| {
            if !matches!(
                current_status(ExternalTool::Ffmpeg),
                ExternalToolStatus::Missing
            ) || !has_prior_managed_install(ExternalTool::Ffmpeg)
            {
                return;
            }
            let name = localized_tool_name(ExternalTool::Ffmpeg);
            let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&name);
            let cancelled = Arc::new(AtomicBool::new(false));
            let Ok(_activity) = crate::install_activity::register(cancelled.clone()) else {
                badge.finish();
                return;
            };
            let result = ensure(ExternalTool::Ffmpeg, &cancelled, |event| {
                report_badge_event(&badge, &name, event);
            });
            badge.finish();
            match result {
                Ok(component) => {
                    drop(component);
                    notify_periodic_update(&name, None);
                }
                Err(error) => notify_periodic_update(&name, Some(&error)),
            }
        },
    ) {
        crate::log_info!("[ExternalTools] periodic_update_not_queued error={error:#}");
    }
}

pub(crate) fn remove(tool: ExternalTool) -> Result<RemovalOutcome> {
    let _tool = lock_tool_mutation(tool);
    super::request_remove_and_wait(tool.id())
}

pub(crate) fn recoveries(tool: ExternalTool) -> Result<Vec<ExternalToolRecovery>> {
    recovery::list(tool)
}

pub(crate) fn clean_recovery(
    tool: ExternalTool,
    recovery: &ExternalToolRecovery,
) -> Result<RecoveryCleanupOutcome> {
    let _local = lock_tool_mutation(tool);
    let _global = super::acquire_mutation_guard()?;
    recovery::clean(tool, recovery)
}

pub(crate) fn purge_all_recorded_recoveries() -> Result<Vec<RecoveryCleanupOutcome>> {
    let _yt_dlp = lock_tool_mutation(ExternalTool::YtDlp);
    let _ffmpeg = lock_tool_mutation(ExternalTool::Ffmpeg);
    let _deno = lock_tool_mutation(ExternalTool::Deno);
    recovery::purge_all_recorded()
}

pub(crate) fn reconcile_interrupted_installs() -> Result<()> {
    let _yt_dlp = lock_tool_mutation(ExternalTool::YtDlp);
    let _ffmpeg = lock_tool_mutation(ExternalTool::Ffmpeg);
    let _deno = lock_tool_mutation(ExternalTool::Deno);
    let _global = super::acquire_mutation_guard()?;
    install::reconcile_interrupted()
}

fn lock_tool_mutation(tool: ExternalTool) -> MutexGuard<'static, ()> {
    TOOL_MUTATION_LOCKS[tool_index(tool)]
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn has_prior_managed_install(tool: ExternalTool) -> bool {
    let Some(current) = delivery_optional(tool) else {
        return false;
    };
    let root = super::components_root().join(tool.id());
    let Ok(metadata) = std::fs::symlink_metadata(&root) else {
        return false;
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.take(64).flatten().any(|entry| {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return false;
        };
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            return false;
        }
        ComponentReceipt::read(&path.join(RECEIPT_NAME))
            .is_ok_and(|receipt| receipt.id == tool.id() && receipt.version != current.version)
    })
}

pub(crate) fn localized_tool_name(tool: ExternalTool) -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&language);
    match tool {
        ExternalTool::YtDlp => text.auxiliary.managed_tools.tool_ytdlp,
        ExternalTool::Ffmpeg => text.auxiliary.managed_tools.tool_ffmpeg,
        ExternalTool::Deno => text.auxiliary.managed_tools.tool_deno,
    }
    .to_string()
}

fn notify_periodic_update(name: &str, error: Option<&anyhow::Error>) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = match error {
        Some(error) => (
            locale.component_install_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        ),
        None => (
            locale.component_installed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        ),
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
}

const fn tool_index(tool: ExternalTool) -> usize {
    match tool {
        ExternalTool::YtDlp => 0,
        ExternalTool::Ffmpeg => 1,
        ExternalTool::Deno => 2,
    }
}

pub(crate) fn webview2_bootstrapper_delivery() -> Result<&'static WebView2BootstrapperDelivery> {
    WEBVIEW2_BOOTSTRAPPER_DELIVERY
        .as_ref()
        .ok_or_else(|| anyhow!("verified WebView2 bootstrapper contract is unavailable"))
}

fn acquire_delivery(
    tool: ExternalTool,
    delivery: &'static ExternalToolDelivery,
) -> Result<ExternalToolUse> {
    validate_install_fast(delivery)?;
    let lease = super::acquire(delivery.id)?;
    let root = std::fs::canonicalize(version_root(delivery)?)?;
    let files = lock_component_files(&root, delivery.files)?;
    validate_install_fast(delivery)?;
    Ok(ExternalToolUse {
        tool,
        root,
        _files: files,
        _lease: lease,
    })
}

fn delivery(tool: ExternalTool) -> Result<&'static ExternalToolDelivery> {
    delivery_optional(tool)
        .ok_or_else(|| anyhow!("verified {} download contract is unavailable", tool.id()))
}

fn delivery_optional(tool: ExternalTool) -> Option<&'static ExternalToolDelivery> {
    debug_assert_eq!(SUPPORTED_ARCHIVE_FORMATS.len(), 2);
    if let Some(delivery) = update::deliveries()
        .iter()
        .find(|delivery| delivery.id == tool.id())
    {
        return Some(delivery);
    }
    EXTERNAL_TOOL_DELIVERIES
        .iter()
        .find(|delivery| delivery.id == tool.id())
}

fn version_root(delivery: &ExternalToolDelivery) -> Result<PathBuf> {
    super::component_version_root(delivery.id, delivery.version)
}

fn validate_install_fast(delivery: &ExternalToolDelivery) -> Result<()> {
    let root = super::validate_version_root(delivery.id, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != delivery.id
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || !receipt.dependencies.is_empty()
        || receipt.files.len() != delivery.files.len()
    {
        bail!("external tool receipt does not match this build");
    }
    for (actual, expected) in receipt.files.iter().zip(delivery.files) {
        let owned = owned_file(expected);
        if !same_file(actual, &owned)
            || !file_metadata_matches(&resolve_owned_path(&root, &owned.path)?, &owned)?
        {
            bail!("external tool failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery.files)
}

fn file_metadata_matches(path: &Path, expected: &OwnedComponentFile) -> Result<bool> {
    let metadata = std::fs::symlink_metadata(path)?;
    Ok(metadata.is_file() && !is_reparse_point(&metadata) && metadata.len() == expected.size_bytes)
}

fn validate_exact_tree(root: &Path, files: &[ExternalToolFile]) -> Result<()> {
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_COMPONENT_FILES + 1)?;
    actual.retain(|path| path != Path::new(RECEIPT_NAME));
    if actual.len() != files.len()
        || actual
            .iter()
            .any(|path| !files.iter().any(|file| Path::new(file.path) == path))
    {
        bail!("external tool contains unowned files");
    }
    Ok(())
}

fn owned_file(file: &ExternalToolFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: file.path.into(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn same_file(left: &OwnedComponentFile, right: &OwnedComponentFile) -> bool {
    left.path == right.path
        && left.size_bytes == right.size_bytes
        && left.sha256.eq_ignore_ascii_case(&right.sha256)
}

fn lock_component_files(root: &Path, files: &[ExternalToolFile]) -> Result<Vec<std::fs::File>> {
    #[cfg(test)]
    RUNTIME_HASH_PASSES.with(|passes| passes.set(passes.get() + 1));
    let mut locked = Vec::with_capacity(files.len());
    for expected in files {
        let path = resolve_owned_path(root, Path::new(expected.path))?;
        let mut file = open_locked_regular_file(&path)?;
        if file.metadata()?.len() != expected.size_bytes {
            bail!("external tool changed while acquiring its use lease");
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected.sha256) {
            bail!("external tool changed while acquiring its use lease");
        }
        locked.push(file);
    }
    Ok(locked)
}

#[cfg(test)]
thread_local! {
    static RUNTIME_HASH_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn reset_runtime_hash_passes() {
    RUNTIME_HASH_PASSES.with(|passes| passes.set(0));
}

#[cfg(test)]
fn runtime_hash_passes() -> usize {
    RUNTIME_HASH_PASSES.with(std::cell::Cell::get)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("external tool use file is unsafe");
    }
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
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("external tool use file is unsafe");
    }
    Ok(file)
}

fn receipt(delivery: &ExternalToolDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: delivery.id.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: Vec::new(),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

#[cfg(test)]
mod tests;
