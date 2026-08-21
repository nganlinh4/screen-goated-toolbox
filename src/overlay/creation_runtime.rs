//! Managed native sidecar shared by creation mini apps.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod capability_probe;
mod download;
mod lifecycle;
mod process_query;
mod update;

pub(crate) use capability_probe::supports_optional_3d_instruction;
pub(crate) use download::{download_runtime, is_runtime_available};
pub(crate) use lifecycle::{shutdown, stop_for_component_removal};

#[derive(Clone, Copy)]
struct RuntimeDelivery {
    version: &'static str,
    features: &'static [&'static str],
    asset: &'static str,
    download_url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/creation_runtime_delivery.rs"));

const COMPONENT_ID: &str = "creation-3d-runtime";

pub(crate) fn download_title() -> String {
    let name = localized_component_name();
    let badge = crate::overlay::auto_copy_badge::locale_text();
    crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_component_fmt,
        &[("name", &name)],
    )
}

fn localized_component_name() -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&language)
        .auxiliary
        .managed_tools
        .tool_creation_engine
        .to_string()
}

fn runtime_version() -> &'static str {
    runtime_delivery()
        .map(|delivery| delivery.version)
        .unwrap_or("not-included")
}

fn runtime_delivery() -> Option<&'static RuntimeDelivery> {
    update::delivery().or(RUNTIME_DELIVERY.as_ref())
}

pub(crate) fn refresh_after_start_failure() -> bool {
    let Some((mode, _, _)) = crate::component_registry::update_catalog::policy(COMPONENT_ID) else {
        return false;
    };
    if mode != "typed-failure" {
        return false;
    }
    let before = runtime_delivery().map(|delivery| (delivery.version, delivery.sha256));
    if crate::component_registry::update_catalog::refresh_now().is_err() {
        return false;
    }
    let after = runtime_delivery().map(|delivery| (delivery.version, delivery.sha256));
    if before.is_none() || after.is_none() || before == after {
        return false;
    }
    invalidate_verified_runtime();
    download_runtime(Arc::new(AtomicBool::new(false)), true).is_ok()
}

fn runtime_component_root() -> PathBuf {
    crate::component_registry::component_version_root(COMPONENT_ID, runtime_version())
        .expect("creation runtime component identity is static and valid")
}

#[cfg(windows)]
struct VerifiedInstalledRuntime {
    version: &'static str,
    path: PathBuf,
    _lease: std::fs::File,
}

#[cfg(windows)]
static VERIFIED_INSTALLED_RUNTIME: LazyLock<Mutex<Option<VerifiedInstalledRuntime>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) fn runtime_bundle_dir() -> PathBuf {
    runtime_component_root().join("bin")
}

fn ensure_runtime_bundle_dir() -> Result<PathBuf> {
    let root = crate::component_registry::ensure_version_root(COMPONENT_ID, runtime_version())?;
    let bin = root.join("bin");
    match std::fs::create_dir(&bin) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(&bin)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("Creation engine directory is not a regular directory.");
    }
    Ok(bin)
}

pub(crate) fn runtime_exe_path() -> PathBuf {
    runtime_bundle_dir().join("sgt_creation_runtime.exe")
}

fn open_runtime_for_validation(path: &Path) -> Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        options.share_mode(FILE_SHARE_READ);
    }
    Ok(options.open(path)?)
}

fn validate_open_runtime(file: &mut std::fs::File) -> Result<()> {
    let delivery = runtime_delivery()
        .ok_or_else(|| anyhow!("Creation engine download contract is unavailable."))?;
    if delivery.version.is_empty() {
        bail!("Creation engine delivery metadata is invalid.");
    }
    let metadata = file
        .metadata()
        .map_err(|error| anyhow!("Creation engine unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != delivery.size_bytes {
        bail!(
            "Creation engine size {} does not match this build.",
            metadata.len(),
        );
    }

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if format!("{:x}", hasher.finalize()) != delivery.sha256 {
        bail!("Creation engine checksum mismatch");
    }
    Ok(())
}

fn validate_runtime(path: &Path) -> Result<()> {
    let mut file = open_runtime_for_validation(path)?;
    validate_open_runtime(&mut file)
}

fn verified_installed_runtime_path() -> Result<PathBuf> {
    migrate_legacy_runtime()?;
    #[cfg(not(windows))]
    {
        let path = runtime_exe_path();
        validate_runtime(&path)?;
        return Ok(path);
    }
    #[cfg(windows)]
    {
        let mut cached = VERIFIED_INSTALLED_RUNTIME
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        let version = runtime_version();
        if let Some(runtime) = cached.as_ref().filter(|runtime| runtime.version == version) {
            return Ok(runtime.path.clone());
        }
        cached.take();
        let path = runtime_exe_path();
        let mut lease = open_runtime_for_validation(&path)?;
        validate_open_runtime(&mut lease)?;
        *cached = Some(VerifiedInstalledRuntime {
            version,
            path: path.clone(),
            _lease: lease,
        });
        Ok(path)
    }
}

fn write_runtime_receipt() -> Result<()> {
    let delivery = runtime_delivery()
        .ok_or_else(|| anyhow!("Creation engine download contract is unavailable."))?;
    crate::component_registry::write_receipt(
        &runtime_component_root(),
        &crate::component_registry::ComponentReceipt {
            schema_version: 1,
            id: COMPONENT_ID.to_string(),
            version: delivery.version.to_string(),
            architecture: "x64".to_string(),
            dependencies: Vec::new(),
            files: vec![crate::component_registry::OwnedComponentFile {
                path: PathBuf::from("bin").join("sgt_creation_runtime.exe"),
                size_bytes: delivery.size_bytes,
                sha256: delivery.sha256.to_string(),
            }],
        },
    )
}

fn migrate_legacy_runtime() -> Result<()> {
    let target = runtime_exe_path();
    if target.is_file() || runtime_delivery().is_none() {
        return Ok(());
    }
    let legacy = crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("bin")
        .join("sgt_creation_runtime.exe");
    if !legacy.is_file() || validate_runtime(&legacy).is_err() {
        return Ok(());
    }
    ensure_runtime_bundle_dir()?;
    std::fs::rename(&legacy, &target)?;
    if let Err(error) = write_runtime_receipt() {
        let _ = std::fs::rename(&target, &legacy);
        return Err(error);
    }
    if let Some(bin) = legacy.parent() {
        let _ = std::fs::remove_dir(bin);
        if let Some(root) = bin.parent() {
            let _ = std::fs::remove_dir(root);
        }
    }
    Ok(())
}

fn invalidate_verified_runtime() {
    #[cfg(windows)]
    {
        VERIFIED_INSTALLED_RUNTIME
            .lock()
            .unwrap_or_else(|value| value.into_inner())
            .take();
    }
}

pub(crate) fn is_runtime_installed() -> bool {
    verified_installed_runtime_path().is_ok()
}

pub(crate) fn remove_runtime() -> Result<()> {
    invalidate_verified_runtime();
    match crate::component_registry::request_remove_and_wait(COMPONENT_ID)? {
        crate::component_registry::RemovalOutcome::Missing => cleanup_runtime_files(true)?,
        crate::component_registry::RemovalOutcome::Removed
        | crate::component_registry::RemovalOutcome::Pending => {}
        crate::component_registry::RemovalOutcome::RequiredBy(dependents) => {
            bail!(
                "Creation engine is still required by {} installed component(s).",
                dependents.len()
            );
        }
        crate::component_registry::RemovalOutcome::PreservedModified(paths) => {
            bail!(
                "Creation engine contains {} modified managed file(s); they were preserved.",
                paths.len()
            );
        }
    }
    Ok(())
}

fn cleanup_runtime_files(include_installed: bool) -> Result<()> {
    let dir = runtime_bundle_dir();
    let Ok(metadata) = std::fs::symlink_metadata(&dir) else {
        return Ok(());
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("Creation engine folder is not a regular directory.");
    }
    for (index, entry) in std::fs::read_dir(&dir)?.enumerate() {
        if index >= 64 {
            bail!("Creation engine folder contains too many entries.");
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let removable =
            include_installed && name == "sgt_creation_runtime.exe" || is_known_partial_name(&name);
        if !removable {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.is_file() && !is_reparse_point(&metadata) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn is_known_partial_name(name: &str) -> bool {
    name.starts_with("sgt_creation_runtime")
        && name.ends_with(".download")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(crate) fn update_installed_runtime_in_background() {
    if runtime_delivery().is_none() {
        return;
    }
    let path = runtime_exe_path();
    if !path.is_file() || is_runtime_installed() {
        return;
    }

    std::thread::spawn(|| {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(error) = download_runtime(stop, true) {
            crate::log_info!("[Creation runtime] Background update failed: {error}");
        }
    });
}

pub(crate) fn shared_runtime_path() -> Option<PathBuf> {
    is_runtime_installed().then(runtime_exe_path)
}

fn supported_readiness_tool(tool: &str) -> bool {
    tool == "3d"
        || tool == "svg" && crate::creation_feature_availability::image_to_svg_release_enabled()
        || tool == "image" && crate::creation_feature_availability::image_creator_release_enabled()
}

fn parse_readiness(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .and_then(|value| {
            let result = value.get("result")?.as_object()?;
            if result.len() != 1 {
                return None;
            }
            match result.get("state")?.as_str()? {
                state @ ("ready" | "preparing" | "unavailable") => Some(state.to_string()),
                _ => None,
            }
        })
}

pub(crate) fn readiness(tool: &str) -> String {
    if !supported_readiness_tool(tool) {
        return "unavailable".to_string();
    }
    let Some(path) = shared_runtime_path() else {
        return "unavailable".to_string();
    };
    let mut command = Command::new(path);
    hide_command_window(&mut command);
    let request = json!({ "id": "readiness", "cmd": "readiness", "args": { "tool": tool } });
    let input = format!("{request}\n");
    process_query::run(
        &mut command,
        Some(input.as_bytes()),
        Duration::from_secs(5),
        64 * 1024,
    )
    .filter(|output| output.status.success() && !output.truncated)
    .and_then(|output| parse_readiness(&output.bytes))
    .unwrap_or_else(|| "unavailable".to_string())
}

const BASE_READINESS_CAPACITY: usize = 4;
const MAX_READINESS_CAPACITY: usize = 6;
const READINESS_RESERVE: usize = 2;

struct ReadinessTask {
    stop: Arc<AtomicBool>,
    desired: AtomicUsize,
    install_if_missing: AtomicBool,
}

static READINESS_IN_FLIGHT: LazyLock<Mutex<std::collections::HashMap<String, Arc<ReadinessTask>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
static RUNTIME_PROCESSES: LazyLock<Mutex<std::collections::HashSet<u32>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));
static RUNTIME_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub(super) fn runtime_shutting_down() -> bool {
    RUNTIME_SHUTTING_DOWN.load(Ordering::Acquire)
}

pub(super) fn register_runtime_process(pid: u32) -> bool {
    if runtime_shutting_down() {
        return false;
    }
    let mut processes = RUNTIME_PROCESSES
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if runtime_shutting_down() {
        return false;
    }
    processes.insert(pid);
    true
}

pub(super) fn unregister_runtime_process(pid: u32) {
    RUNTIME_PROCESSES
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .remove(&pid);
}

pub(crate) fn cancel_readiness(tool: &str) {
    if !supported_readiness_tool(tool) {
        return;
    }
    if let Some(task) = READINESS_IN_FLIGHT
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .remove(tool)
    {
        task.stop.store(true, Ordering::Release);
    }
}

fn remove_readiness_if_current(
    in_flight: &mut std::collections::HashMap<String, Arc<ReadinessTask>>,
    tool: &str,
    task: &Arc<ReadinessTask>,
) {
    if in_flight
        .get(tool)
        .is_some_and(|current| Arc::ptr_eq(current, task))
    {
        in_flight.remove(tool);
    }
}

fn desired_readiness_capacity(active_demand: usize) -> usize {
    active_demand
        .saturating_add(READINESS_RESERVE)
        .clamp(BASE_READINESS_CAPACITY, MAX_READINESS_CAPACITY)
}

fn send_live_capacity_request(tool: String, desired: usize) {
    std::thread::spawn(move || {
        let Some(path) = shared_runtime_path() else {
            return;
        };
        let desired = desired.to_string();
        let mut command = Command::new(path);
        command.args([
            "--request-readiness-capacity",
            "--tool",
            &tool,
            "--desired-capacity",
            &desired,
        ]);
        let _ = process_query::run(&mut command, None, Duration::from_secs(5), 16 * 1024);
    });
}

pub(crate) fn maintain_readiness_for_demand(
    tool: &str,
    active_demand: usize,
    install_if_missing: bool,
) {
    if !supported_readiness_tool(tool) {
        return;
    }
    let desired = desired_readiness_capacity(active_demand);
    let task;
    {
        let mut in_flight = READINESS_IN_FLIGHT
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if let Some(current) = in_flight.get(tool) {
            current.desired.fetch_max(desired, Ordering::AcqRel);
            if install_if_missing {
                current.install_if_missing.store(true, Ordering::Release);
            }
            send_live_capacity_request(tool.to_string(), desired);
            return;
        }
        task = Arc::new(ReadinessTask {
            stop: Arc::new(AtomicBool::new(false)),
            desired: AtomicUsize::new(desired),
            install_if_missing: AtomicBool::new(install_if_missing),
        });
        in_flight.insert(tool.to_string(), task.clone());
    }
    let tool = tool.to_string();
    std::thread::spawn(move || {
        loop {
            let desired = task.desired.swap(0, Ordering::AcqRel);
            if shared_runtime_path().is_none() && task.install_if_missing.load(Ordering::Acquire) {
                let _ = download_runtime(task.stop.clone(), true);
            }
            if task.stop.load(Ordering::Acquire) {
                break;
            }
            let Some(path) = shared_runtime_path() else {
                break;
            };
            let desired = desired.max(BASE_READINESS_CAPACITY).to_string();
            let parent_pid = std::process::id().to_string();
            let mut command = Command::new(path);
            command.args([
                "--maintain-readiness",
                "--tool",
                &tool,
                "--desired-capacity",
                &desired,
                "--parent-pid",
                &parent_pid,
                "--headless",
            ]);
            hide_command_window(&mut command);
            let _ = process_query::run_cancellable(
                &mut command,
                None,
                Duration::from_secs(30 * 60),
                64 * 1024,
                || task.stop.load(Ordering::Acquire),
            );
            if task.desired.load(Ordering::Acquire) == 0 {
                break;
            }
        }
        remove_readiness_if_current(
            &mut READINESS_IN_FLIGHT
                .lock()
                .unwrap_or_else(|value| value.into_inner()),
            &tool,
            &task,
        );
    });
}

pub(crate) fn maintain_readiness(tool: &str, install_if_missing: bool) {
    maintain_readiness_for_demand(tool, 0, install_if_missing);
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

#[cfg(test)]
mod tests;
