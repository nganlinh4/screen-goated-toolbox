use std::path::Path;

use anyhow::Result;

#[cfg(windows)]
pub(super) fn stop_exact_executable(executable: &Path) -> Result<()> {
    windows_impl::stop_exact_executable(executable)
}

#[cfg(not(windows))]
pub(super) fn stop_exact_executable(_executable: &Path) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
mod windows_impl {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    const STOP_TIMEOUT: Duration = Duration::from_secs(30);
    const STOP_INTERVAL: Duration = Duration::from_millis(20);

    pub(super) fn stop_exact_executable(executable: &Path) -> Result<()> {
        let target = comparable_path(executable);
        let mut matching = matching_processes(&target)?;
        if matching.is_empty() {
            return Ok(());
        }

        for pid in matching.drain(..) {
            crate::overlay::creation_recovery::terminate_process_tree(pid);
        }

        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            let remaining = matching_processes(&target)?;
            if remaining.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!(
                    "processes using '{}' did not stop before cleanup timed out: {:?}",
                    executable.display(),
                    remaining
                );
            }
            std::thread::sleep(STOP_INTERVAL);
        }
    }

    fn matching_processes(target: &str) -> Result<Vec<u32>> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .context("enumerate processes before downloaded-tool removal")?;
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut matches = Vec::new();

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;
                    if process_path(pid)
                        .as_deref()
                        .is_some_and(|path| comparable_path(path) == target)
                    {
                        matches.push(pid);
                    }
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
            Ok(matches)
        }
    }

    fn process_path(pid: u32) -> Option<PathBuf> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);
            result
                .is_ok()
                .then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize])))
        }
    }

    fn comparable_path(path: &Path) -> String {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let display = path.to_string_lossy().replace('/', "\\");
        display
            .strip_prefix(r"\\?\")
            .unwrap_or(&display)
            .to_ascii_lowercase()
    }
}
