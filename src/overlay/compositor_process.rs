use anyhow::{Context, Result, bail};
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::process::Child;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
    QueryInformationJobObject, TerminateJobObject,
};

const GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_millis(750);
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Keep an independent handle so exit status survives generation replacement
/// and termination of the renderer's entire browser job.
pub(crate) fn observe_exit(child: &Child, role: &'static str, generation: u64) -> Result<()> {
    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
    let borrowed =
        unsafe { std::os::windows::io::BorrowedHandle::borrow_raw(child.as_raw_handle()) };
    let process = borrowed
        .try_clone_to_owned()
        .context("duplicate renderer diagnostic handle")?;
    let pid = child.id();
    std::thread::Builder::new().name(format!("{role}-exit")).spawn(move || {
        let handle = HANDLE(process.as_raw_handle());
        let mut code = 0;
        if unsafe { WaitForSingleObject(handle, INFINITE) } == WAIT_OBJECT_0
            && unsafe { GetExitCodeProcess(handle, &mut code) }.is_ok()
        {
            crate::crash_report::record_event(&format!("[RendererExit] role={role} generation={generation} pid={pid} exit_code=0x{code:08X}"));
        }
        drop(process);
    }).context("start renderer exit observer")?;
    Ok(())
}

/// Reusing a browser profile requires every process in its owning job to exit,
/// not just the compositor that launched the browser.
pub(crate) fn retire_renderer_tree(child: &mut Child, job: &OwnedHandle) -> Result<()> {
    wait_for_exit_or_kill(child);
    let handle = HANDLE(job.as_raw_handle());
    unsafe { TerminateJobObject(handle, 0) }.context("terminate compositor browser job")?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(handle),
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast(),
                std::mem::size_of_val(&accounting) as u32,
                None,
            )
        }
        .context("query compositor browser job")?;
        if accounting.ActiveProcesses == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("compositor browser job has not exited");
        }
        std::thread::sleep(EXIT_POLL_INTERVAL);
    }
}

pub(crate) fn watchdog_observation_stale(previous_ms: u64, now_ms: u64, timeout_ms: u64) -> bool {
    now_ms.saturating_sub(previous_ms) > timeout_ms
}

/// Wait briefly after a compositor has received its shutdown command, then
/// retain forceful termination as a bounded failure fallback.
pub(crate) fn wait_for_exit_or_kill(child: &mut Child) {
    let deadline = Instant::now() + GRACEFUL_EXIT_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(EXIT_POLL_INTERVAL),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn watchdog_should_restart(
    desired: bool,
    transitioning: bool,
    live_generation: u64,
) -> bool {
    desired && !transitioning && live_generation == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn paused_supervisor_requires_a_new_observation_window() {
        assert!(!super::watchdog_observation_stale(1000, 2000, 5000));
        assert!(!super::watchdog_observation_stale(1000, 6000, 5000));
        assert!(super::watchdog_observation_stale(1000, 6001, 5000));
    }
    #[test]
    fn watchdog_never_restarts_an_intentional_lifecycle_transition() {
        assert!(!super::watchdog_should_restart(true, true, 0));
        assert!(!super::watchdog_should_restart(false, false, 0));
        assert!(!super::watchdog_should_restart(true, false, 7));
        assert!(super::watchdog_should_restart(true, false, 0));
    }
}
