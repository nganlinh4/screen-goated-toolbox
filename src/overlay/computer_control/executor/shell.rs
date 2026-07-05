//! Shell / launch handlers for the Computer Control executor: run a PowerShell
//! command (the general escape hatch) and open a URL or launch an app via
//! `ShellExecuteW`. Fully self-contained — no shared `SendInput` state.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

/// Run a PowerShell command (non-interactive, no profile) and capture its text
/// output — the agent's GENERAL escape hatch for anything without a dedicated
/// tool (files, processes, volume, system info). Inherits THIS process's
/// (non-elevated) privileges. `CREATE_NO_WINDOW` avoids a console flash.
pub(super) fn run_command(args: &Value) -> Result<Value> {
    let command = args
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing command"))?;
    if let Some(why) = catastrophic_block(command) {
        return Ok(json!({
            "ok": false,
            "refused": true,
            "reason": format!("Refused (safety stop): this looks like {why}, which is irreversible and \
        system-destroying. If the user truly intends it, they should run it themselves."),
        }));
    }
    // Run on a worker thread with a hard timeout: a command that waits for input or genuinely hangs
    // must NEVER freeze the agent forever (the abandoned process just finishes in the background).
    let cmd = command.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(exec_powershell(&cmd));
    });
    match rx.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(r) => r,
        Err(_) => Ok(json!({
            "ok": false,
            "timed_out": true,
            "error": "the command did not finish within 60s (it may be waiting for input or hung) - \
        make it non-interactive, or break it into smaller steps",
        })),
    }
}

/// Spawn PowerShell (non-interactive, no profile) and capture its output. `CREATE_NO_WINDOW`
/// avoids a console flash. Inherits THIS process's (possibly elevated) privileges.
fn exec_powershell(command: &str) -> Result<Value> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| anyhow!("failed to launch powershell: {e}"))?;
    let clip = |b: &[u8]| -> String {
        String::from_utf8_lossy(b)
            .trim()
            .chars()
            .take(4000)
            .collect()
    };
    Ok(json!({
        "ok": output.status.success(),
        "exit_code": output.status.code(),
        "stdout": clip(&output.stdout),
        "stderr": clip(&output.stderr),
    }))
}

/// A TINY hard-stop list: ONLY the irreversible, system-destroying patterns the agent must never run
/// unprompted — formatting/wiping a disk, killing a core OS process (instant bugcheck), or deleting a
/// Windows system root. Everything else runs freely (smooth UX); the model asks before merely-unexpected
/// ops per its prompt. Read-only commands (e.g. `Get-Process lsass`) are NOT blocked — only the kill.
fn catastrophic_block(cmd: &str) -> Option<&'static str> {
    let c = cmd.to_ascii_lowercase();
    if c.contains("format-volume") || c.contains("clear-disk") || c.contains("diskpart") {
        return Some("formatting / wiping a disk");
    }
    let killing =
        c.contains("stop-process") || c.contains("taskkill") || c.contains("kill-process");
    if killing
        && ["csrss", "wininit", "winlogon", "lsass", "smss"]
            .iter()
            .any(|p| c.contains(p))
    {
        return Some("killing a core OS process (would crash Windows)");
    }
    let deleting = c.contains("remove-item") || c.contains("rmdir") || c.contains("del ");
    if deleting && (c.contains("\\windows") || c.contains("system32")) {
        return Some("deleting a Windows system directory");
    }
    None
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
