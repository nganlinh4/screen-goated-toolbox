//! Bounded session.log file: append, high-watermark compaction, and the
//! fallback that keeps logging when compaction cannot replace the file.
use anyhow::{Context, Result, anyhow};
use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};
#[cfg(windows)]
use windows::core::PCWSTR;

const LOG_MAX_BYTES: u64 = 16 * 1024 * 1024;
const LOG_RETAIN_BYTES: u64 = 12 * 1024 * 1024;
pub(super) const MAX_LOG_LINE_BYTES: usize = 64 * 1024;
const COMPACTION_MARKER: &[u8] =
    b"[Log] Earlier entries removed because session.log exceeded its size limit.\n";
const COMPACTION_TEMP_NAME: &str = ".session.log.compacting";
const COMPACTION_RETRY_FIRST: Duration = Duration::from_secs(5);
const COMPACTION_RETRY_MAX: Duration = Duration::from_secs(5 * 60);

/// Retry schedule for a compaction that failed, for example because a viewer
/// holds session.log open without delete sharing. Lives as long as the writer
/// thread so the deferral survives across batches.
#[derive(Default)]
pub(super) struct CompactionBackoff {
    retry_at: Option<Instant>,
    delay: Option<Duration>,
}

impl CompactionBackoff {
    fn ready(&self, now: Instant) -> bool {
        self.retry_at.is_none_or(|retry_at| now >= retry_at)
    }

    fn defer(&mut self, now: Instant) -> Duration {
        let delay = self
            .delay
            .map_or(COMPACTION_RETRY_FIRST, |delay| delay * 2)
            .min(COMPACTION_RETRY_MAX);
        self.delay = Some(delay);
        self.retry_at = Some(now + delay);
        delay
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

fn open_writer(path: &Path) -> Result<BufWriter<File>> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open session log {}", path.display()))?;
    Ok(BufWriter::new(file))
}

pub(super) struct CompactingWriter<'a> {
    path: &'a Path,
    writer: Option<BufWriter<File>>,
    bytes_written: u64,
    max_bytes: u64,
    retain_bytes: u64,
    backoff: &'a mut CompactionBackoff,
}

impl<'a> CompactingWriter<'a> {
    pub(super) fn open(path: &'a Path, backoff: &'a mut CompactionBackoff) -> Result<Self> {
        Self::open_with_limits(path, LOG_MAX_BYTES, LOG_RETAIN_BYTES, backoff)
    }

    fn open_with_limits(
        path: &'a Path,
        max_bytes: u64,
        retain_bytes: u64,
        backoff: &'a mut CompactionBackoff,
    ) -> Result<Self> {
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
            backoff,
        })
    }

    pub(super) fn write_line(&mut self, line: &str) -> Result<()> {
        let line = bounded_line(line, MAX_LOG_LINE_BYTES);
        let line_bytes = line.len() as u64 + 1;
        if self.bytes_written.saturating_add(line_bytes) > self.max_bytes {
            self.compact_or_defer()?;
        }
        self.append(&line)
    }

    fn append(&mut self, line: &str) -> Result<()> {
        if self.writer.is_none() {
            self.writer = Some(open_writer(self.path)?);
        }
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("session log writer unavailable"))?;
        writeln!(writer, "{line}").context("write session log line")?;
        self.bytes_written = self.bytes_written.saturating_add(line.len() as u64 + 1);
        Ok(())
    }

    /// A failed compaction must not cost log lines: keep appending past the cap
    /// and retry later, leaving one marker per deferral.
    fn compact_or_defer(&mut self) -> Result<()> {
        let now = Instant::now();
        if !self.backoff.ready(now) {
            return Ok(());
        }
        match self.compact() {
            Ok(()) => {
                self.backoff.reset();
                Ok(())
            }
            Err(error) => {
                let delay = self.backoff.defer(now);
                self.append(&format!(
                    "[Log] session.log compaction deferred for {}s; appending past its size limit: {error:#}",
                    delay.as_secs()
                ))
            }
        }
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

    pub(super) fn flush(&mut self) -> Result<()> {
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

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "session_file_tests.rs"]
mod tests;
