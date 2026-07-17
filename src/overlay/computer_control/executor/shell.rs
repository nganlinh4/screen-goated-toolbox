//! Shell / launch handlers for the Computer Control executor: run a PowerShell
//! command (the general escape hatch) and open a URL or launch an app via
//! `ShellExecuteW`. Fully self-contained — no shared `SendInput` state.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

#[path = "shell_contract.rs"]
mod contract;

/// Run a PowerShell command (non-interactive, no profile) and capture its text
/// output — the agent's GENERAL escape hatch for anything without a dedicated
/// tool (files, processes, volume, system info). Inherits THIS process's
/// (non-elevated) privileges. `CREATE_NO_WINDOW` avoids a console flash.
pub(super) fn run_command(args: &Value) -> Result<Value> {
    run_command_with_timeout(args, Duration::from_secs(60))
}

fn run_command_with_timeout(args: &Value, timeout: Duration) -> Result<Value> {
    if let Some(spec) = ProcessSpec::parse(args)? {
        if let Some(rejection) = contract::inline_interpreter_rejection(&spec.program, &spec.args) {
            return Ok(rejection);
        }
        return run_process_with_timeout(spec, timeout);
    }
    let command = command_arg(args)?;
    let child = spawn_powershell(command)?;
    let pid = child.child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(wait_powershell(child));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let termination = terminate_process_tree(pid);
            // Reap the worker after termination when possible. The bounded wait
            // prevents a broken OS process handle from becoming a new hang.
            let reaped = rx.recv_timeout(Duration::from_secs(5)).is_ok();
            Ok(json!({
                "ok": false,
                "timed_out": true,
                "effect_verified": false,
                "effect_may_have_occurred": true,
                "terminated": termination.is_ok(),
                "reaped": reaped,
                "termination_error": termination.err().map(|error| error.to_string()),
                "error": format!(
                    "the command exceeded {}ms and its process tree was terminated",
                    timeout.as_millis()
                ),
            }))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow!("PowerShell worker stopped without a result"))
        }
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
            program: resolve_program(program, &cwd),
            args: argv,
            cwd,
        }))
    }
}

fn resolve_program(program: &str, cwd: &Path) -> String {
    let requested = Path::new(program);
    let explicit_path = requested.is_absolute() || requested.components().count() > 1;
    let mut bases = Vec::new();
    if explicit_path {
        bases.push(if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            cwd.join(requested)
        });
    } else {
        bases.push(cwd.join(requested));
        if let Some(path) = std::env::var_os("PATH") {
            bases.extend(
                std::env::split_paths(&path)
                    .filter(|path| !path.as_os_str().is_empty())
                    .map(|path| path.join(requested)),
            );
        }
    }
    for base in bases {
        if requested.extension().is_some() {
            if base.is_file() {
                return base.to_string_lossy().into_owned();
            }
            continue;
        }
        for extension in windows_program_extensions() {
            let candidate = base.with_extension(extension);
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        if explicit_path && base.is_file() {
            return base.to_string_lossy().into_owned();
        }
    }
    program.to_string()
}

fn windows_program_extensions() -> Vec<String> {
    let configured = std::env::var("PATHEXT").unwrap_or_default();
    let extensions = configured
        .split(';')
        .map(|extension| {
            extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase()
        })
        .filter(|extension| matches!(extension.as_str(), "com" | "exe" | "bat" | "cmd"))
        .collect::<Vec<_>>();
    if extensions.is_empty() {
        vec!["com".into(), "exe".into(), "bat".into(), "cmd".into()]
    } else {
        extensions
    }
}

fn run_process_with_timeout(spec: ProcessSpec, timeout: Duration) -> Result<Value> {
    let child = spawn_process(&spec)?;
    let pid = child.child.id();
    let worker_spec = spec.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(wait_process(child, &worker_spec));
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let termination = terminate_process_tree(pid);
            let reaped = rx.recv_timeout(Duration::from_secs(5)).is_ok();
            Ok(process_timeout_value(
                &spec,
                timeout,
                termination.is_ok(),
                reaped,
                termination.err().map(|error| error.to_string()),
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow!("exact process worker stopped without a result"))
        }
    }
}

fn command_arg(args: &Value) -> Result<&str> {
    args.get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing command"))
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn spawn_process(spec: &ProcessSpec) -> Result<CapturedChild> {
    use std::os::windows::process::CommandExt;
    let (capture, stdout, stderr) = CommandCapture::create()?;
    let child = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| anyhow!("failed to launch exact process: {error}"))?;
    Ok(CapturedChild { child, capture })
}

/// Spawn PowerShell without waiting so the caller retains a PID that can be
/// terminated structurally at the timeout boundary.
fn spawn_powershell(command: &str) -> Result<CapturedChild> {
    use std::os::windows::process::CommandExt;
    let (capture, stdout, stderr) = CommandCapture::create()?;
    let cwd = command_working_dir()?;
    let child = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| anyhow!("failed to launch powershell: {error}"))?;
    Ok(CapturedChild { child, capture })
}

fn command_working_dir() -> Result<PathBuf> {
    let path = crate::paths::app_temp_dir().join("cc-command-work");
    std::fs::create_dir_all(&path)
        .map_err(|error| anyhow!("failed to create managed command working directory: {error}"))?;
    std::fs::canonicalize(&path)
        .map_err(|error| anyhow!("failed to resolve managed command working directory: {error}"))
}

fn wait_powershell(mut child: CapturedChild) -> Result<Value> {
    let status = child
        .child
        .wait()
        .map_err(|error| anyhow!("failed to wait for powershell: {error}"))?;
    let output = child.capture.output(status)?;
    Ok(output_value(&output))
}

fn wait_process(mut child: CapturedChild, spec: &ProcessSpec) -> Result<Value> {
    let status = child
        .child
        .wait()
        .map_err(|error| anyhow!("failed to wait for exact process: {error}"))?;
    let output = child.capture.output(status)?;
    Ok(process_output_value(&output, spec))
}

struct CapturedChild {
    child: Child,
    capture: CommandCapture,
}

struct CommandCapture {
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

impl CommandCapture {
    fn create() -> Result<(Self, File, File)> {
        static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);
        for _ in 0..64 {
            let id = NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed);
            let stem = format!("sgt-cc-command-{}-{id}", std::process::id());
            let stdout_path = std::env::temp_dir().join(format!("{stem}.stdout"));
            let stderr_path = std::env::temp_dir().join(format!("{stem}.stderr"));
            let stdout = match create_capture_file(&stdout_path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(anyhow!("failed to create command stdout capture: {error}"));
                }
            };
            let stderr = match create_capture_file(&stderr_path) {
                Ok(file) => file,
                Err(error) => {
                    let _ = std::fs::remove_file(&stdout_path);
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        continue;
                    }
                    return Err(anyhow!("failed to create command stderr capture: {error}"));
                }
            };
            return Ok((
                Self {
                    stdout_path,
                    stderr_path,
                },
                stdout,
                stderr,
            ));
        }
        Err(anyhow!("could not allocate command output capture files"))
    }

    fn output(&self, status: ExitStatus) -> Result<Output> {
        Ok(Output {
            status,
            stdout: std::fs::read(&self.stdout_path)
                .map_err(|error| anyhow!("failed to read command stdout: {error}"))?,
            stderr: std::fs::read(&self.stderr_path)
                .map_err(|error| anyhow!("failed to read command stderr: {error}"))?,
        })
    }
}

impl Drop for CommandCapture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.stdout_path);
        let _ = std::fs::remove_file(&self.stderr_path);
    }
}

fn create_capture_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn output_value(output: &Output) -> Value {
    let (stdout, stdout_truncated) = captured_text(&output.stdout);
    let (stderr, stderr_truncated) = captured_text(&output.stderr);
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
    })
}

fn process_output_value(output: &Output, spec: &ProcessSpec) -> Value {
    let (stdout, stdout_truncated) = captured_text(&output.stdout);
    let (stderr, stderr_truncated) = captured_text(&output.stderr);
    let mut exact = vec![
        "/program",
        "/args",
        "/arg_count",
        "/cwd",
        "/exit_code",
        "/process_completed",
    ];
    let mut partial = Vec::new();
    if stdout_truncated {
        partial.push("/stdout");
    } else {
        exact.push("/stdout");
    }
    if stderr_truncated {
        partial.push("/stderr");
    } else {
        exact.push("/stderr");
    }
    json!({
        "ok": output.status.success(),
        "ok_scope": "exact_process_exit_status",
        "evidence_kind": "exact_process_invocation",
        "program": &spec.program,
        "args": &spec.args,
        "arg_count": spec.args.len(),
        "cwd": spec.cwd.to_string_lossy(),
        "exit_code": output.status.code(),
        "process_completed": true,
        "effect_verified": false,
        "effect_may_have_occurred": true,
        "stdout": stdout,
        "stderr": stderr,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "completion_proof": {
            "exact": exact,
            "partial": partial,
        },
    })
}

fn process_timeout_value(
    spec: &ProcessSpec,
    timeout: Duration,
    terminated: bool,
    reaped: bool,
    termination_error: Option<String>,
) -> Value {
    json!({
        "ok": false,
        "ok_scope": "exact_process_exit_status",
        "evidence_kind": "exact_process_invocation",
        "program": &spec.program,
        "args": &spec.args,
        "arg_count": spec.args.len(),
        "cwd": spec.cwd.to_string_lossy(),
        "timed_out": true,
        "effect_verified": false,
        "effect_may_have_occurred": true,
        "terminated": terminated,
        "reaped": reaped,
        "termination_error": termination_error,
        "error": format!(
            "the process exceeded {}ms and its process tree was terminated",
            timeout.as_millis()
        ),
        "completion_proof": {
            "exact": ["/program", "/args", "/arg_count", "/cwd", "/timed_out", "/terminated", "/reaped"],
        },
    })
}

fn captured_text(bytes: &[u8]) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    let value = trimmed.chars().take(4_000).collect::<String>();
    (value, trimmed.chars().count() > 4_000)
}

fn terminate_process_tree(pid: u32) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let output = Command::new("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| anyhow!("failed to launch taskkill: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "taskkill failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
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
#[path = "shell_tests.rs"]
mod tests;
