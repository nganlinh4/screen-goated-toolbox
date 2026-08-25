use anyhow::{Context, Result, anyhow};
use chrono::Local;
use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};
#[cfg(windows)]
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
#[cfg(windows)]
use windows::core::PCWSTR;

const LOG_MAX_BYTES: u64 = 16 * 1024 * 1024;
const LOG_RETAIN_BYTES: u64 = 12 * 1024 * 1024;
const MAX_LOG_LINE_BYTES: usize = 64 * 1024;
const MAX_BATCH_LINES: usize = 1024;
const COMPACTION_MARKER: &[u8] =
    b"[Log] Earlier entries removed because session.log exceeded its size limit.\n";
const COMPACTION_TEMP_NAME: &str = ".session.log.compacting";
#[cfg(windows)]
const LOG_WRITE_MUTEX: &str = "Local\\ScreenGoatedToolboxSessionLogWrite-v1";
#[cfg(windows)]
const LOG_WRITE_WAIT_MS: u32 = 5_000;

static STDOUT_RESERVED_FOR_PROTOCOL: LazyLock<bool> =
    LazyLock::new(|| stdout_reserved_for_protocol(std::env::args_os().skip(1)));

static LOG_SENDER: LazyLock<Sender<String>> = LazyLock::new(|| {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("sgt-log-writer".to_string())
        .spawn(move || writer_loop(receiver))
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
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let _ = LOG_SENDER.send(format!("[{timestamp}] {msg}"));
}

fn writer_loop(receiver: Receiver<String>) {
    let mut path = crate::paths::app_sgt_dir();
    path.push("logs");
    let _ = std::fs::create_dir_all(&path);
    path.push("session.log");

    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                write_available(&path, &receiver, line);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return;
            }
        }
    }
}

fn write_available(path: &Path, receiver: &Receiver<String>, first: String) {
    let Ok(_guard) = LogWriteGuard::acquire() else {
        return;
    };
    let Ok(mut writer) = CompactingWriter::open(path) else {
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

fn open_writer(path: &Path) -> Result<BufWriter<File>> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open session log {}", path.display()))?;
    Ok(BufWriter::new(file))
}

struct CompactingWriter<'a> {
    path: &'a Path,
    writer: Option<BufWriter<File>>,
    bytes_written: u64,
    max_bytes: u64,
    retain_bytes: u64,
}

impl<'a> CompactingWriter<'a> {
    fn open(path: &'a Path) -> Result<Self> {
        Self::open_with_limits(path, LOG_MAX_BYTES, LOG_RETAIN_BYTES)
    }

    fn open_with_limits(path: &'a Path, max_bytes: u64, retain_bytes: u64) -> Result<Self> {
        if retain_bytes <= COMPACTION_MARKER.len() as u64 || retain_bytes >= max_bytes {
            return Err(anyhow!("invalid session log compaction limits"));
        }
        remove_if_exists(&compaction_temp_path(path))?;
        let bytes_written = file_len(path)?;
        Ok(Self {
            path,
            writer: Some(open_writer(path)?),
            bytes_written,
            max_bytes,
            retain_bytes,
        })
    }

    fn write_line(&mut self, line: &str) -> Result<()> {
        let line = bounded_line(line, MAX_LOG_LINE_BYTES);
        let line_bytes = line.len() as u64 + 1;
        if self.bytes_written.saturating_add(line_bytes) > self.max_bytes {
            self.compact()?;
        }
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("session log writer unavailable"))?;
        writeln!(writer, "{line}").context("write session log line")?;
        self.bytes_written = self.bytes_written.saturating_add(line_bytes);
        Ok(())
    }

    fn compact(&mut self) -> Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer
                .flush()
                .context("flush session log before compaction")?;
        }
        self.bytes_written = compact_log(self.path, self.retain_bytes)?;
        self.writer = Some(open_writer(self.path)?);
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if let Some(writer) = self.writer.as_mut() {
            writer.flush().context("flush session log")?;
        }
        Ok(())
    }
}

fn bounded_line(line: &str, max_bytes: usize) -> Cow<'_, str> {
    if line.len() <= max_bytes {
        return Cow::Borrowed(line);
    }
    let suffix = format!(" ... [truncated; original_bytes={}]", line.len());
    if suffix.len() >= max_bytes {
        return Cow::Owned(suffix[..max_bytes].to_string());
    }
    let mut prefix_len = max_bytes - suffix.len();
    while !line.is_char_boundary(prefix_len) {
        prefix_len -= 1;
    }
    Cow::Owned(format!("{}{}", &line[..prefix_len], suffix))
}

fn compact_log(path: &Path, retain_bytes: u64) -> Result<u64> {
    let tail_budget = retain_bytes
        .checked_sub(COMPACTION_MARKER.len() as u64)
        .ok_or_else(|| anyhow!("session log retain limit is too small"))?;
    let mut source = File::open(path)
        .with_context(|| format!("open session log for compaction {}", path.display()))?;
    let source_len = source.metadata().context("read session log size")?.len();
    let start = source_len.saturating_sub(tail_budget);
    source
        .seek(SeekFrom::Start(start))
        .context("seek to retained session log tail")?;
    let mut tail = Vec::with_capacity(tail_budget as usize);
    (&mut source)
        .take(tail_budget)
        .read_to_end(&mut tail)
        .context("read retained session log tail")?;
    let complete_line_start = if start == 0 {
        0
    } else {
        tail.iter()
            .position(|byte| *byte == b'\n')
            .map_or(tail.len(), |position| position + 1)
    };
    drop(source);

    let temp_path = compaction_temp_path(path);
    remove_if_exists(&temp_path)?;
    let result = write_compacted_file(&temp_path, &tail[complete_line_start..])
        .and_then(|()| replace_file(path, &temp_path));
    if result.is_err() {
        let _ = remove_if_exists(&temp_path);
    }
    result?;
    file_len(path)
}

fn write_compacted_file(path: &Path, tail: &[u8]) -> Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create compacted session log {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(COMPACTION_MARKER)
        .and_then(|()| writer.write_all(tail))
        .context("write compacted session log")?;
    writer.flush().context("flush compacted session log")?;
    writer
        .get_ref()
        .sync_all()
        .context("sync compacted session log")
}

fn compaction_temp_path(path: &Path) -> PathBuf {
    path.with_file_name(COMPACTION_TEMP_NAME)
}

fn file_len(path: &Path) -> Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => {
            Err(error).with_context(|| format!("read session log metadata {}", path.display()))
        }
    }
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("remove stale log file {}", path.display()))
        }
    }
}

#[cfg(windows)]
fn replace_file(destination: &Path, replacement: &Path) -> Result<()> {
    let destination = wide_path(destination);
    let replacement = wide_path(replacement);
    unsafe {
        ReplaceFileW(
            PCWSTR(destination.as_ptr()),
            PCWSTR(replacement.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
    .context("atomically replace compacted session log")
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(not(windows))]
fn replace_file(destination: &Path, replacement: &Path) -> Result<()> {
    remove_if_exists(destination)?;
    std::fs::rename(replacement, destination).context("replace compacted session log")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

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

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sgt-debug-log-{label}-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn log_path(&self) -> PathBuf {
            self.0.join("session.log")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn oversized_lines_are_utf8_safe_and_bounded() {
        let line = "한".repeat(30_000);
        let bounded = bounded_line(&line, MAX_LOG_LINE_BYTES);

        assert!(bounded.len() <= MAX_LOG_LINE_BYTES);
        assert!(bounded.contains("[truncated; original_bytes=90000]"));
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
    }

    #[test]
    fn compaction_keeps_recent_complete_lines_in_one_file() {
        let dir = TestDir::new("compaction");
        let path = dir.log_path();
        let content = (0..40)
            .map(|index| format!("line-{index:02}-abcdefghij\n"))
            .collect::<String>();
        std::fs::write(&path, content).unwrap();

        let compacted_bytes = compact_log(&path, 180).unwrap();
        let compacted = std::fs::read_to_string(&path).unwrap();

        assert!(compacted_bytes <= 180);
        assert!(compacted.starts_with(std::str::from_utf8(COMPACTION_MARKER).unwrap()));
        assert!(!compacted.contains("line-00"));
        assert!(compacted.contains("line-39"));
        assert!(!compaction_temp_path(&path).exists());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn writer_compacts_at_high_watermark_and_stays_bounded() {
        let dir = TestDir::new("watermark");
        let path = dir.log_path();
        let mut writer = CompactingWriter::open_with_limits(&path, 320, 200).unwrap();

        for index in 0..20 {
            writer
                .write_line(&format!("entry-{index:02}-abcdefghijklmnop"))
                .unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() <= 320);
        assert!(content.contains("Earlier entries removed"));
        assert!(content.contains("entry-19"));
        assert!(!compaction_temp_path(&path).exists());
    }
}
