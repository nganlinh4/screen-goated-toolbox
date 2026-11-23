//! Managed native sidecar shared by Creation mini apps.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};

mod capability_probe;
mod download;
mod lifecycle;
mod process_query;
mod readiness_cache;
pub(crate) mod verification;

pub(crate) use capability_probe::supports_optional_3d_instruction;
pub(crate) use download::download_runtime;
pub(crate) use lifecycle::{shutdown, stop_for_component_removal};

pub(crate) fn configured_command(path: impl AsRef<std::ffi::OsStr>) -> std::io::Result<Command> {
    let mut command = Command::new(path);
    crate::paths::configure_isolated_child_environment(&mut command)?;
    Ok(command)
}

struct VerifiedInstalledRuntime {
    runtime: crate::component_registry::creation::RuntimeUse,
}

static VERIFIED_INSTALLED_RUNTIME: LazyLock<Mutex<Option<VerifiedInstalledRuntime>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) fn refresh_after_start_failure() -> bool {
    invalidate_verified_runtime();
    crate::component_registry::creation::refresh_after_start_failure()
}

fn verified_installed_runtime_path() -> Result<PathBuf> {
    let mut cached = VERIFIED_INSTALLED_RUNTIME
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if let Some(runtime) = cached.as_ref() {
        return Ok(runtime.runtime.path().to_path_buf());
    }
    let runtime = crate::component_registry::creation::open_runtime()?;
    let path = runtime.path().to_path_buf();
    *cached = Some(VerifiedInstalledRuntime { runtime });
    Ok(path)
}

fn invalidate_verified_runtime() {
    VERIFIED_INSTALLED_RUNTIME
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .take();
}

pub(crate) fn remove_runtime() -> Result<()> {
    invalidate_verified_runtime();
    crate::component_registry::creation::remove()
}

pub(crate) fn shared_runtime_path() -> Option<PathBuf> {
    verified_installed_runtime_path().ok()
}

fn supported_readiness_tool(tool: &str) -> bool {
    match tool {
        "3d" => crate::component_registry::creation::supports("image_to_3d"),
        "svg" => {
            crate::creation_feature_availability::image_to_svg_release_enabled()
                && crate::component_registry::creation::supports("image_to_svg")
        }
        _ => false,
    }
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
    let observed = readiness_cache::read(tool);
    let preparing = READINESS_IN_FLIGHT
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .get(tool)
        .is_some_and(|task| !task.stop.load(Ordering::Acquire));
    displayed_readiness(&observed, preparing).to_string()
}

fn displayed_readiness(observed: &str, preparing: bool) -> &str {
    if preparing && observed == "unavailable" {
        "preparing"
    } else {
        observed
    }
}

fn query_readiness(tool: &str) -> String {
    if !supported_readiness_tool(tool) {
        return "unavailable".to_string();
    }
    let Some(path) = shared_runtime_path() else {
        return "unavailable".to_string();
    };
    let Ok(mut command) = configured_command(path) else {
        return "unavailable".to_string();
    };
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

/// Warm runtime slots kept per Creation tool, whatever the queue depth.
const READINESS_CAPACITY: usize = 3;

struct ReadinessTask {
    stop: Arc<AtomicBool>,
    /// Set by a later request while the maintainer runs, so it runs once more.
    rerun: AtomicBool,
    install_if_missing: AtomicBool,
}

static READINESS_IN_FLIGHT: LazyLock<Mutex<std::collections::HashMap<String, Arc<ReadinessTask>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
static RUNTIME_PROCESSES: LazyLock<
    Mutex<std::collections::HashMap<u32, crate::overlay::creation_process::Termination>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
static RUNTIME_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub(super) fn runtime_shutting_down() -> bool {
    RUNTIME_SHUTTING_DOWN.load(Ordering::Acquire)
}

pub(super) fn register_runtime_process(
    child: &crate::overlay::creation_process::CreationProcess,
) -> bool {
    if runtime_shutting_down() {
        return false;
    }
    let mut processes = RUNTIME_PROCESSES
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if runtime_shutting_down() {
        return false;
    }
    processes.insert(child.id(), child.termination());
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

fn send_live_capacity_request(tool: String, desired: usize) {
    std::thread::spawn(move || {
        let Some(path) = shared_runtime_path() else {
            return;
        };
        let desired = desired.to_string();
        let Ok(mut command) = configured_command(path) else {
            return;
        };
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

pub(crate) fn maintain_readiness(tool: &str, install_if_missing: bool) {
    if !supported_readiness_tool(tool) {
        return;
    }
    let desired = READINESS_CAPACITY;
    let task;
    {
        let mut in_flight = READINESS_IN_FLIGHT
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if let Some(current) = in_flight.get(tool) {
            current.rerun.store(true, Ordering::Release);
            if install_if_missing {
                current.install_if_missing.store(true, Ordering::Release);
            }
            send_live_capacity_request(tool.to_string(), desired);
            return;
        }
        task = Arc::new(ReadinessTask {
            stop: Arc::new(AtomicBool::new(false)),
            rerun: AtomicBool::new(false),
            install_if_missing: AtomicBool::new(install_if_missing),
        });
        in_flight.insert(tool.to_string(), task.clone());
    }
    let tool = tool.to_string();
    std::thread::spawn(move || {
        loop {
            task.rerun.store(false, Ordering::Release);
            if shared_runtime_path().is_none()
                && task.install_if_missing.load(Ordering::Acquire)
                && let Err(error) = download_runtime(task.stop.clone(), true)
            {
                crate::log_info!("[Creation] Runtime preparation failed: {error:#}");
            }
            if task.stop.load(Ordering::Acquire) {
                break;
            }
            let Some(path) = shared_runtime_path() else {
                break;
            };
            let desired = desired.to_string();
            let parent_pid = std::process::id().to_string();
            let Ok(mut command) = configured_command(path) else {
                break;
            };
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
            if !task.rerun.load(Ordering::Acquire) {
                if tool == "3d" {
                    verification::refresh();
                }
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

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

#[cfg(test)]
mod tests;
