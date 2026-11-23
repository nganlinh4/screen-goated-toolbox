//! Coordinate replacement startup with the exact process that requested it.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

pub(super) const PARENT_MARKER: &str = "SGT_WEBVIEW2_RESTART_PARENT";
// Keep the existing environment key as the replacement startup contract.
const WAIT_TIMEOUT_MS: u32 = 30_000;

fn creation_time(process: HANDLE) -> Result<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }?;
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

pub(super) fn current_process_marker() -> std::io::Result<String> {
    let created = creation_time(unsafe { GetCurrentProcess() }).map_err(std::io::Error::other)?;
    Ok(format!("{}:{created}", std::process::id()))
}

fn parse_marker(marker: &str) -> Result<(u32, u64)> {
    let (pid, created) = marker
        .split_once(':')
        .context("invalid restart parent marker")?;
    let pid: u32 = pid.parse().context("invalid restart parent process ID")?;
    let created: u64 = created
        .parse()
        .context("invalid restart parent creation time")?;
    if pid == 0 || created == 0 {
        bail!("invalid restart parent identity");
    }
    Ok((pid, created))
}

fn wait_for_parent(pid: u32, expected_creation: u64, timeout_ms: u32) -> Result<()> {
    let raw = match unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    } {
        Ok(handle) => handle,
        // Windows reports ERROR_INVALID_PARAMETER for a PID that no longer
        // exists. Access denial or any other uncertainty must fail closed.
        Err(error) if error.code().0 as u32 == 0x80070057 => return Ok(()),
        Err(error) => return Err(error).context("open restart parent"),
    };
    let process = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let handle = HANDLE(process.as_raw_handle());
    if creation_time(handle)? != expected_creation {
        // The original process has exited and Windows has reused its PID.
        return Ok(());
    }
    match unsafe { WaitForSingleObject(handle, timeout_ms) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => bail!("previous application instance did not exit"),
        _ => bail!("could not wait for previous application instance"),
    }
}

/// Runs at process entry, before singleton acquisition or any background work.
pub(crate) fn await_previous_process() -> Result<()> {
    let Some(marker) = std::env::var_os(PARENT_MARKER) else {
        return Ok(());
    };
    // Process entry runs before this application starts any threads.
    unsafe { std::env::remove_var(PARENT_MARKER) };
    let marker = marker
        .to_str()
        .context("invalid restart parent marker encoding")?;
    let (pid, created) = parse_marker(marker)?;
    wait_for_parent(pid, created, WAIT_TIMEOUT_MS)
}

#[cfg(test)]
#[path = "restart_native_tests.rs"]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_identity_requires_a_real_pid_and_creation_time() {
        assert_eq!(parse_marker("123:456").unwrap(), (123, 456));
        for invalid in ["", "0:1", "1:0", "1", "1:2:3", "abc:1"] {
            assert!(parse_marker(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn waiting_is_bound_to_process_creation_not_pid_alone() {
        let pid = std::process::id();
        let created = creation_time(unsafe { GetCurrentProcess() }).unwrap();
        assert!(wait_for_parent(pid, created, 0).is_err());
        assert!(wait_for_parent(pid, created.wrapping_add(1), 0).is_ok());
    }

    #[test]
    fn waiting_finishes_only_after_the_owned_process_exits() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        use std::time::Duration;

        const CHILD_MARKER: &str = "SGT_RESTART_WAIT_TEST_CHILD";
        if std::env::var_os(CHILD_MARKER).is_some() {
            println!("sgt-restart-wait-ready");
            std::io::stdout().flush().unwrap();
            let _ = std::io::stdin().read_line(&mut String::new());
            return;
        }

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime_support::restart::tests::waiting_finishes_only_after_the_owned_process_exits",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut notified = false;
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if !notified && line.contains("sgt-restart-wait-ready") {
                    let _ = ready_tx.send(());
                    notified = true;
                }
            }
        });
        if ready_rx.recv_timeout(Duration::from_secs(10)).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            reader.join().unwrap();
            panic!("restart wait test child did not become ready");
        }
        let created = creation_time(HANDLE(child.as_raw_handle())).unwrap();
        let pid = child.id();
        let (done_tx, done_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            done_tx.send(wait_for_parent(pid, created, 10_000)).unwrap();
        });
        let early = done_rx.recv_timeout(Duration::from_millis(50));
        child.stdin.take().unwrap().write_all(b"\n").unwrap();
        let (waited_for_exit, result) = match early {
            Ok(result) => (false, result),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                (true, done_rx.recv_timeout(Duration::from_secs(10)).unwrap())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.wait();
                panic!("restart waiter disconnected");
            }
        };
        let status = child.wait().unwrap();
        waiter.join().unwrap();
        reader.join().unwrap();
        assert!(waited_for_exit);
        assert!(result.is_ok());
        assert!(status.success());
    }
}
