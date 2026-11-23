use anyhow::{Context, Result, anyhow};
use chrono::Local;
#[cfg(not(feature = "recorder-worker"))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::mpsc::{Receiver, Sender, channel};

#[cfg(not(feature = "recorder-worker"))]
pub(crate) mod diagnostics;
// Explicit path: the recorder worker includes this file through `#[path]`.
#[path = "debug_log/session_file.rs"]
mod session_file;

use session_file::{CompactingWriter, CompactionBackoff};

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
#[cfg(windows)]
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
#[cfg(windows)]
use windows::core::PCWSTR;

const MAX_BATCH_LINES: usize = 1024;
#[cfg(windows)]
const LOG_WRITE_MUTEX: &str = "Local\\ScreenGoatedToolboxSessionLogWrite-v1";
#[cfg(windows)]
const LOG_WRITE_WAIT_MS: u32 = 5_000;

#[cfg(not(feature = "recorder-worker"))]
static STDOUT_RESERVED_FOR_PROTOCOL: LazyLock<bool> =
    LazyLock::new(|| stdout_reserved_for_protocol(std::env::args_os().skip(1)));

static LOG_SENDER: LazyLock<Sender<String>> = LazyLock::new(|| {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("sgt-log-writer".to_string())
        .spawn(move || writer_loop(receiver, &session_log_path()))
        .expect("failed to start SGT log writer");
    sender
});

pub fn print_line(msg: &str) {
    #[cfg(feature = "recorder-worker")]
    {
        eprintln!("{msg}");
    }

    #[cfg(not(feature = "recorder-worker"))]
    {
        if *STDOUT_RESERVED_FOR_PROTOCOL {
            return;
        }
        #[cfg(windows)]
        if write_console_line(msg) {
            return;
        }
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{msg}");
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn stdout_reserved_for_protocol(arguments: impl Iterator<Item = std::ffi::OsString>) -> bool {
    arguments
        .filter_map(|argument| argument.into_string().ok())
        .any(|argument| argument.starts_with("--internal-") && argument.ends_with("-compositor"))
}

#[cfg(all(windows, not(feature = "recorder-worker")))]
fn write_console_line(msg: &str) -> bool {
    use windows::Win32::System::Console::{
        CONSOLE_MODE, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE, WriteConsoleW,
    };

    let Ok(handle) = (unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }) else {
        return false;
    };
    let mut mode = CONSOLE_MODE::default();
    if unsafe { GetConsoleMode(handle, &mut mode) }.is_err() {
        return false;
    }
    let wide: Vec<u16> = format!("{msg}\r\n").encode_utf16().collect();
    unsafe { WriteConsoleW(handle, &wide, None, None) }.is_ok()
}

pub fn log_debug(msg: &str) {
    // Unit tests exercise production call sites, but must not evict customer
    // history from the real bounded session log.
    if cfg!(test) {
        return;
    }
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let _ = LOG_SENDER.send(format!("[{timestamp}] {msg}"));
}

fn session_log_path() -> PathBuf {
    let mut path = crate::paths::app_sgt_dir();
    path.push("logs");
    let _ = std::fs::create_dir_all(&path);
    path.push("session.log");
    path
}

/// Blocks until a line arrives; there is no idle wakeup. The loop ends when
/// every sender is gone.
fn writer_loop(receiver: Receiver<String>, path: &Path) {
    let mut backoff = CompactionBackoff::default();
    while let Ok(line) = receiver.recv() {
        write_available(path, &receiver, line, &mut backoff);
    }
}

fn write_available(
    path: &Path,
    receiver: &Receiver<String>,
    first: String,
    backoff: &mut CompactionBackoff,
) {
    let Ok(_guard) = LogWriteGuard::acquire() else {
        return;
    };
    let Ok(mut writer) = CompactingWriter::open(path, backoff) else {
        return;
    };
    if writer.write_line(&first).is_err() {
        return;
    }
    for line in receiver.try_iter().take(MAX_BATCH_LINES - 1) {
        if writer.write_line(&line).is_err() {
            return;
        }
    }
    let _ = writer.flush();
}

#[cfg(windows)]
struct LogWriteGuard(HANDLE);

#[cfg(windows)]
impl LogWriteGuard {
    fn acquire() -> Result<Self> {
        let name = LOG_WRITE_MUTEX
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .context("create session log write mutex")?;
        let wait = unsafe { WaitForSingleObject(handle, LOG_WRITE_WAIT_MS) };
        if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
            return Ok(Self(handle));
        }
        unsafe {
            let _ = CloseHandle(handle);
        }
        Err(anyhow!("session log write mutex wait failed: {}", wait.0))
    }
}

#[cfg(windows)]
impl Drop for LogWriteGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
struct LogWriteGuard;

#[cfg(not(windows))]
impl LogWriteGuard {
    fn acquire() -> Result<Self> {
        Ok(Self)
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        {
            let msg = format!($($arg)*);
            $crate::debug_log::print_line(&msg);
            $crate::debug_log::log_debug(&msg);
        }
    };
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    #[test]
    fn unit_test_logging_does_not_start_the_customer_log_writer() {
        log_debug("unit test diagnostic");
        assert!(LazyLock::get(&LOG_SENDER).is_none());
    }

    #[test]
    fn internal_compositors_keep_stdout_exclusive_to_protocol_frames() {
        assert!(stdout_reserved_for_protocol(
            ["--internal-result-compositor".into()].into_iter()
        ));
        assert!(stdout_reserved_for_protocol(
            ["--internal-status-compositor".into()].into_iter()
        ));
        assert!(!stdout_reserved_for_protocol(
            ["--result-compositor-smoke".into()].into_iter()
        ));
    }

    #[test]
    fn writer_loop_drains_queued_lines_and_exits_when_senders_close() {
        let dir = std::env::temp_dir().join(format!(
            "sgt-debug-log-loop-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.log");
        let (sender, receiver) = channel();
        let writer = std::thread::spawn({
            let path = path.clone();
            move || writer_loop(receiver, &path)
        });
        sender.send("first line".to_string()).unwrap();
        sender.send("second line".to_string()).unwrap();
        drop(sender);
        writer.join().unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(content.contains("first line\n") && content.contains("second line\n"));
    }
}
