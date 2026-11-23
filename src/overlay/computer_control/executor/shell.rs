//! Shell / launch handlers for the Computer Control executor: run a PowerShell
//! command (the general escape hatch) and open a URL or launch an app via
//! `ShellExecuteW`. Fully self-contained — no shared `SendInput` state.

use anyhow::{Result, anyhow};
use base64::Engine as _;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

#[path = "program_resolution.rs"]
mod program_resolution;
use program_resolution::{native_fallback, resolve_program};

#[path = "shell_capture.rs"]
mod capture;
#[path = "shell_contract.rs"]
mod contract;
#[path = "shell_script.rs"]
mod script;
#[path = "shell_wait.rs"]
mod wait;
use super::super::shell_job as job;
use capture::{CapturedOutput, CommandCapture, MAX_CAPTURE_BYTES, Reservation};
use wait::{WaitResult, interrupted_value, process_interrupted_value, wait_cancellable};

/// Run a PowerShell command (non-interactive, no profile) and capture its text
/// output — the agent's GENERAL escape hatch for anything without a dedicated
/// tool (files, processes, volume, system info). Inherits THIS process's
/// (non-elevated) privileges. `CREATE_NO_WINDOW` avoids a console flash.
pub(super) fn run_command(args: &Value, cancel: &AtomicBool) -> Result<Value> {
    run_command_cancellable(args, Duration::from_secs(60), cancel)
}

pub(in crate::overlay::computer_control) fn scope_context(args: &Value) -> Result<Value> {
    if let Some(spec) = ProcessSpec::parse(args)? {
        Ok(
            json!({"mode":"exact_process", "program":spec.program, "args":spec.args, "cwd":spec.cwd}),
        )
    } else {
        command_arg(args)?;
        Ok(json!({"mode":"powershell", "cwd":command_working_dir()?}))
    }
}

#[cfg(test)]
fn run_command_with_timeout(args: &Value, timeout: Duration) -> Result<Value> {
    run_command_cancellable(args, timeout, &AtomicBool::new(false))
}

fn run_command_cancellable(args: &Value, timeout: Duration, cancel: &AtomicBool) -> Result<Value> {
    if cancel.load(Ordering::SeqCst) {
        let mut result = interrupted_value("cancelled", false, false, false, None);
        result["terminated"] = json!(false);
        result["termination_requested"] = json!(false);
        result["not_started"] = json!(true);
        return Ok(result);
    }
    let spec = ProcessSpec::parse(args)?;
    if let Some(spec) = &spec
        && let Some(rejection) = contract::inline_interpreter_rejection(&spec.program, &spec.args)
    {
        return Ok(rejection);
    }
    let command = if spec.is_none() {
        Some(command_arg(args)?)
    } else {
        None
    };
    let reservation = match Reservation::acquire() {
        Ok(reservation) => reservation,
        Err(_) => {
            return Ok(json!({
                "ok": false,
                "status": "resource_unavailable",
                "error": "command output reader capacity is exhausted",
                "retryable": true,
                "effect_may_have_occurred": false,
            }));
        }
    };
    if let Some(spec) = spec {
        return run_process_with_timeout(spec, timeout, cancel, reservation);
    }
    match wait_cancellable(
        spawn_powershell(command.unwrap(), reservation)?,
        timeout,
        cancel,
    )? {
        WaitResult::Complete(output) => Ok(output_value(&output)),
        WaitResult::Interrupted(value) => Ok(value),
    }
}

#[derive(Clone)]
struct ProcessSpec {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
}

impl ProcessSpec {
    fn parse(args: &Value) -> Result<Option<Self>> {
        let program = args.get("program");
        let command = args.get("command");
        if program.is_some() && command.is_some() {
            return Err(anyhow!("supply program or command, not both"));
        }
        let Some(program) = program else {
            if command.is_none() {
                return Err(anyhow!("missing program or command"));
            }
            if args.get("args").is_some() || args.get("cwd").is_some() {
                return Err(anyhow!("args and cwd require exact program mode"));
            }
            return Ok(None);
        };
        let program = program
            .as_str()
            .map(str::trim)
            .filter(|program| !program.is_empty() && program.len() <= 1_024)
            .ok_or_else(|| anyhow!("program must be a non-empty string up to 1024 bytes"))?;
        let argv = args
            .get("args")
            .map(|value| {
                let values = value
                    .as_array()
                    .filter(|values| values.len() <= 16)
                    .ok_or_else(|| anyhow!("args must be an array of at most 16 strings"))?;
                let mut total = 0usize;
                values
                    .iter()
                    .map(|value| {
                        let value = value
                            .as_str()
                            .filter(|value| value.len() <= 4_096)
                            .ok_or_else(|| {
                                anyhow!("each process argument must be a string up to 4096 bytes")
                            })?;
                        total = total.saturating_add(value.len());
                        if total > 16_384 {
                            return Err(anyhow!("process arguments exceed 16384 bytes"));
                        }
                        Ok(value.to_string())
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let cwd = match args.get("cwd") {
            Some(value) => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| anyhow!("cwd must be an absolute directory path"))?;
                let path = PathBuf::from(raw);
                if !path.is_absolute() || !path.is_dir() {
                    return Err(anyhow!("cwd must be an existing absolute directory"));
                }
                std::fs::canonicalize(&path)
                    .map_err(|error| anyhow!("failed to resolve process cwd: {error}"))?
            }
            None => command_working_dir()?,
        };
        Ok(Some(Self {
            program: resolve_program(program, &cwd)?,
            args: argv,
            cwd,
        }))
    }
}

fn run_process_with_timeout(
    spec: ProcessSpec,
    timeout: Duration,
    cancel: &AtomicBool,
    reservation: Reservation,
) -> Result<Value> {
    match wait_cancellable(spawn_process(&spec, reservation)?, timeout, cancel)? {
        WaitResult::Complete(output) => Ok(process_output_value(&output, &spec)),
        WaitResult::Interrupted(value) => Ok(process_interrupted_value(value, &spec)),
    }
}

fn command_arg(args: &Value) -> Result<&str> {
    args.get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing command"))
}

/// Every exact process is created suspended inside its kill-on-close job in
/// one native call, leaving no instant where a creator crash orphans the child.
/// A batch script runs through `cmd.exe` with Rust std's command line, and a
/// bare name not found on PATH uses std's remaining native search locations.
fn spawn_process(spec: &ProcessSpec, reservation: Reservation) -> Result<CapturedChild> {
    let program = launch_path(&spec.program)?;
    let mut command = if is_batch_script(&program) {
        let processor = crate::system_command::system_executable("cmd.exe")?;
        job::HelperCommand::batch_script(processor, &program)
    } else {
        job::HelperCommand::new(&program)
    };
    command.args(&spec.args).current_dir(&spec.cwd);
    capture_child(job::ContainedChild::spawn_atomic(&command)?, reservation)
}

fn launch_path(program: &str) -> Result<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    native_fallback(program).ok_or_else(|| anyhow!("program not found: {program}"))
}

fn is_batch_script(program: &Path) -> bool {
    program.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
    })
}

/// Spawn PowerShell without waiting so the caller retains a PID that can be
/// terminated structurally at the timeout boundary.
fn spawn_powershell(command: &str, reservation: Reservation) -> Result<CapturedChild> {
    let cwd = command_working_dir()?;
    let executable = crate::system_command::system_relative_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])?;
    let mut shell = job::HelperCommand::new(executable);
    shell
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(script::powershell_script(command))
        .current_dir(cwd);
    capture_child(job::ContainedChild::spawn_atomic(&shell)?, reservation)
}

fn capture_child(
    mut child: job::ContainedChild,
    reservation: Reservation,
) -> Result<CapturedChild> {
    let stdout = child
        .take_stdout()
        .ok_or_else(|| anyhow!("command stdout pipe is missing"))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| anyhow!("command stderr pipe is missing"))?;
    let capture = CommandCapture::start(stdout, stderr, reservation)?;
    Ok(CapturedChild { child, capture })
}

fn command_working_dir() -> Result<PathBuf> {
    let path = crate::paths::app_temp_dir().join("cc-command-work");
    std::fs::create_dir_all(&path)
        .map_err(|error| anyhow!("failed to create managed command working directory: {error}"))?;
    std::fs::canonicalize(&path)
        .map_err(|error| anyhow!("failed to resolve managed command working directory: {error}"))
}

struct CapturedChild {
    child: job::ContainedChild,
    capture: CommandCapture,
}

fn output_value(capture: &CapturedOutput) -> Value {
    let output = &capture.output;
    let (stdout, stdout_truncated, stdout_lossy) = captured_text(&output.stdout);
    let (stderr, stderr_truncated, stderr_lossy) = captured_text(&output.stderr);
    let (stdout_bytes, stdout_bytes_complete) =
        captured_bytes(&output.stdout, capture.stdout_complete);
    let (stderr_bytes, stderr_bytes_complete) =
        captured_bytes(&output.stderr, capture.stderr_complete);
    json!({
        "ok": output.status.success(),
        "ok_scope": "process_exit_status",
        "exit_code": output.status.code(),
        "process_completed": true,
        "effect_verified": false,
        "effect_may_have_occurred": true,
        "stdout": stdout,
        "stderr": stderr,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "stdout_lossy": stdout_lossy,
        "stderr_lossy": stderr_lossy,
        "stdout_encoding": "unknown_or_mixed",
        "stderr_encoding": "unknown_or_mixed",
        "stdout_text_exact": false,
        "stderr_text_exact": false,
        "stdout_bytes_base64": stdout_bytes,
        "stderr_bytes_base64": stderr_bytes,
        "stdout_bytes_complete": stdout_bytes_complete,
        "stderr_bytes_complete": stderr_bytes_complete,
        "stdout_complete": capture.stdout_complete,
        "stderr_complete": capture.stderr_complete,
    })
}

fn process_output_value(capture: &CapturedOutput, spec: &ProcessSpec) -> Value {
    let output = &capture.output;
    let (stdout, stdout_truncated, stdout_lossy) = captured_text(&output.stdout);
    let (stderr, stderr_truncated, stderr_lossy) = captured_text(&output.stderr);
    let (stdout_bytes, stdout_bytes_complete) =
        captured_bytes(&output.stdout, capture.stdout_complete);
    let (stderr_bytes, stderr_bytes_complete) =
        captured_bytes(&output.stderr, capture.stderr_complete);
    let cwd = spec.cwd.to_string_lossy();
    let cwd_lossy = matches!(cwd, std::borrow::Cow::Owned(_));
    let mut exact = vec![
        "/program",
        "/args",
        "/arg_count",
        "/exit_code",
        "/process_completed",
    ];
    let mut partial = vec!["/stdout", "/stderr"];
    if cwd_lossy {
        partial.push("/cwd");
    } else {
        exact.push("/cwd");
    }
    if stdout_bytes_complete {
        exact.push("/stdout_bytes_base64");
    } else {
        partial.push("/stdout_bytes_base64");
    }
    if stderr_bytes_complete {
        exact.push("/stderr_bytes_base64");
    } else {
        partial.push("/stderr_bytes_base64");
    }
    json!({
        "ok": output.status.success(),
        "ok_scope": "exact_process_exit_status",
        "evidence_kind": "exact_process_invocation",
        "program": &spec.program,
        "args": &spec.args,
        "arg_count": spec.args.len(),
        "cwd": cwd,
        "cwd_lossy": cwd_lossy,
        "exit_code": output.status.code(),
        "process_completed": true,
        "effect_verified": false,
        "effect_may_have_occurred": true,
        "stdout": stdout,
        "stderr": stderr,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "stdout_lossy": stdout_lossy,
        "stderr_lossy": stderr_lossy,
        "stdout_encoding": "unknown_or_mixed",
        "stderr_encoding": "unknown_or_mixed",
        "stdout_text_exact": false,
        "stderr_text_exact": false,
        "stdout_bytes_base64": stdout_bytes,
        "stderr_bytes_base64": stderr_bytes,
        "stdout_bytes_complete": stdout_bytes_complete,
        "stderr_bytes_complete": stderr_bytes_complete,
        "stdout_complete": capture.stdout_complete,
        "stderr_complete": capture.stderr_complete,
        "completion_proof": {
            "exact": exact,
            "partial": partial,
        },
    })
}

const RAW_PREFIX_BYTES: usize = 1024;

fn captured_bytes(bytes: &[u8], complete: bool) -> (String, bool) {
    let prefix = &bytes[..bytes.len().min(RAW_PREFIX_BYTES)];
    (
        base64::engine::general_purpose::STANDARD.encode(prefix),
        complete && bytes.len() <= RAW_PREFIX_BYTES,
    )
}

fn captured_text(bytes: &[u8]) -> (String, bool, bool) {
    let byte_truncated = bytes.len() > MAX_CAPTURE_BYTES;
    let input = &bytes[..bytes.len().min(MAX_CAPTURE_BYTES)];
    let lossy = std::str::from_utf8(input).is_err();
    let text = String::from_utf8_lossy(input);
    let value = text.chars().take(4_000).collect::<String>();
    (value, byte_truncated || text.chars().count() > 4_000, lossy)
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `ShellExecuteW "open"` on a file/app/URL, optionally with command-line
/// arguments (e.g. open a file in an app). Returns Ok if the shell accepted it
/// (HINSTANCE > 32 per the Win32 contract).
fn shell_open(file: &str, params: Option<&str>) -> Result<()> {
    let op = to_wide("open");
    let file_w = to_wide(file);
    let params_w = params.filter(|p| !p.is_empty()).map(to_wide);
    let params_ptr = params_w
        .as_ref()
        .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr()));
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file_w.as_ptr()),
            params_ptr,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = r.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(anyhow!("ShellExecuteW failed (code {code})"))
    }
}

/// Open an http(s) URL in the default browser (a new, foreground tab). Far more
/// reliable than driving the address bar by keystrokes.
pub(super) fn open_url(args: &Value) -> Result<Value> {
    let url = http_url_arg(args)?;
    shell_open(url, None)?;
    Ok(json!({"ok": true, "opened_url": url}))
}

fn http_url_arg(args: &Value) -> Result<&str> {
    let raw = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing url"))?
        .trim();
    let valid = url::Url::parse(raw)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if valid {
        Ok(raw)
    } else {
        Err(anyhow!("url must be an absolute http:// or https:// URL"))
    }
}

/// Launch (or focus) an application by name/path via the shell, e.g. "chrome",
/// "notepad", "explorer", with optional arguments (e.g. open a file in an app:
/// name="notepad", args="C:\path\file.txt"). More reliable than the Win+type
/// Start-menu dance.
pub(super) fn launch_app(args: &Value) -> Result<Value> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing name"))?;
    let app_args = args.get("args").and_then(Value::as_str);
    shell_open(name, app_args)?;
    Ok(json!({"ok": true, "launched": name, "args": app_args}))
}

#[cfg(test)]
#[path = "shell_cancellation_tests.rs"]
mod cancellation_tests;
#[cfg(test)]
#[path = "shell_evidence_tests.rs"]
mod evidence_tests;
#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
