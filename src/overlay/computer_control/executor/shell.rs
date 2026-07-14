//! Shell / launch handlers for the Computer Control executor: run a PowerShell
//! command (the general escape hatch) and open a URL or launch an app via
//! `ShellExecuteW`. Fully self-contained — no shared `SendInput` state.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

/// Run a PowerShell command (non-interactive, no profile) and capture its text
/// output — the agent's GENERAL escape hatch for anything without a dedicated
/// tool (files, processes, volume, system info). Inherits THIS process's
/// (non-elevated) privileges. `CREATE_NO_WINDOW` avoids a console flash.
pub(super) fn run_command(args: &Value) -> Result<Value> {
    run_command_with_timeout(args, Duration::from_secs(60))
}

fn run_command_with_timeout(args: &Value, timeout: Duration) -> Result<Value> {
    let command = command_arg(args)?;
    let child = spawn_powershell(command)?;
    let pid = child.id();
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

fn command_arg(args: &Value) -> Result<&str> {
    args.get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing command"))
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Spawn PowerShell without waiting so the caller retains a PID that can be
/// terminated structurally at the timeout boundary.
fn spawn_powershell(command: &str) -> Result<Child> {
    use std::os::windows::process::CommandExt;
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| anyhow!("failed to launch powershell: {error}"))
}

fn wait_powershell(child: Child) -> Result<Value> {
    let output = child
        .wait_with_output()
        .map_err(|error| anyhow!("failed to wait for powershell: {error}"))?;
    Ok(output_value(&output))
}

fn output_value(output: &Output) -> Value {
    let clip = |b: &[u8]| -> String {
        String::from_utf8_lossy(b)
            .trim()
            .chars()
            .take(4000)
            .collect()
    };
    json!({
        "ok": output.status.success(),
        "exit_code": output.status.code(),
        "stdout": clip(&output.stdout),
        "stderr": clip(&output.stderr),
    })
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
    let url = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing url"))?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(anyhow!("url must start with http:// or https://"));
    }
    shell_open(url, None)?;
    Ok(json!({"ok": true, "opened_url": url}))
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
mod tests {
    use super::{command_arg, run_command_with_timeout};
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[test]
    fn command_content_is_never_classified_by_substring() {
        for command in [
            "Write-Output 'alpha beta'",
            "Get-Thing | Set-Thing -Mode 'Mixed-Case'",
            "Invoke-Tool 'C:\\path with spaces\\item'",
        ] {
            assert_eq!(command_arg(&json!({"command": command})).unwrap(), command);
        }
    }

    #[test]
    fn command_field_is_still_structurally_required() {
        assert!(command_arg(&json!({})).is_err());
        assert!(command_arg(&json!({"command": 1})).is_err());
    }

    #[test]
    fn timeout_terminates_and_reaps_the_process_tree() {
        let started = Instant::now();
        let result = run_command_with_timeout(
            &json!({"command": "Start-Sleep -Seconds 20"}),
            Duration::from_millis(100),
        )
        .unwrap();
        assert_eq!(result["timed_out"], true);
        assert_eq!(result["terminated"], true);
        assert_eq!(result["reaped"], true);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
