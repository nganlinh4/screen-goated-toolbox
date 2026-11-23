use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, UnlockFileEx,
};
use windows::Win32::System::IO::OVERLAPPED;

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const RETAIN_BYTES: u64 = 12 * 1024 * 1024;
const MARKER: &[u8] = b"{\"event\":\"telemetry_compacted\",\"earlier_records_omitted\":true}\n";
/// A session trace keeps its beginning (session start, goal and first actions)
/// and stops growing at this size; later records are replaced by one marker.
const SESSION_MAX_BYTES: u64 = 64 * 1024 * 1024;
const SESSION_CAP_MARKER: &[u8] =
    b"{\"event\":\"telemetry_session_capped\",\"later_records_omitted\":true}\n";

#[derive(Default)]
pub(super) struct Writers {
    global: Option<Sink>,
    session: Option<Sink>,
}

impl Writers {
    pub(super) fn append(&mut self, path: &Path, line: &[u8], global: bool) -> io::Result<()> {
        let slot = if global {
            &mut self.global
        } else {
            &mut self.session
        };
        if slot.as_ref().is_none_or(|sink| sink.path != path) {
            *slot = None;
            *slot = Some(Sink::open(path)?);
        }
        let sink = slot.as_mut().expect("opened telemetry sink");
        if global {
            sink.append(line, Some((MAX_BYTES, RETAIN_BYTES)))
        } else {
            sink.append_capped(line, SESSION_MAX_BYTES)
        }
    }
}

struct Sink {
    path: PathBuf,
    file: File,
    _directories: Vec<File>,
}

impl Sink {
    fn open(path: &Path) -> io::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("trace has no parent"))?;
        if !parent.is_absolute() {
            return Err(io::Error::other("trace parent is not absolute"));
        }
        let mut ancestors = parent.ancestors().collect::<Vec<_>>();
        ancestors.reverse();
        let mut directories = Vec::new();
        for directory in ancestors {
            match std::fs::symlink_metadata(directory) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    match std::fs::create_dir(directory) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            }
            let handle = OpenOptions::new()
                .read(true)
                .access_mode(FILE_READ_ATTRIBUTES.0)
                .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(directory)?;
            let metadata = handle.metadata()?;
            if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other("unsafe trace directory"));
            }
            directories.push(handle);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("unsafe trace file"));
        }
        Ok(Self {
            path: path.to_owned(),
            file,
            _directories: directories,
        })
    }

    fn append(&mut self, line: &[u8], limits: Option<(u64, u64)>) -> io::Result<()> {
        self.append_bounded(line, limits, None)
    }

    /// Append without rewriting earlier records: once `cap` would be exceeded,
    /// write a single cap marker and drop later records.
    fn append_capped(&mut self, line: &[u8], cap: u64) -> io::Result<()> {
        self.append_bounded(line, None, Some(cap))
    }

    fn append_bounded(
        &mut self,
        line: &[u8],
        limits: Option<(u64, u64)>,
        cap: Option<u64>,
    ) -> io::Result<()> {
        if let Some((max, retain)) = limits
            && (retain + MARKER.len() as u64 >= max
                || line.len() as u64 + 1 > max - retain - MARKER.len() as u64)
        {
            return Err(io::Error::other(
                "global telemetry record exceeds its size budget",
            ));
        }
        let _lock = FileLock::acquire(&self.file)?;
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(self.file.as_raw_handle()), &mut information) }
            .map_err(io::Error::other)?;
        if information.nNumberOfLinks != 1 {
            return Err(io::Error::other("trace file has multiple links"));
        }
        let mut length = self.file.metadata()?.len();
        if length > 0 {
            self.file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            self.file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                // Repair an interrupted append without concatenating records.
                let budget = limits.map_or(RETAIN_BYTES, |(_, retain)| retain);
                let start = length.saturating_sub(budget);
                self.file.seek(SeekFrom::Start(start))?;
                let mut tail = Vec::new();
                Read::by_ref(&mut self.file)
                    .take(budget)
                    .read_to_end(&mut tail)?;
                if let Some(index) = tail.iter().rposition(|byte| *byte == b'\n') {
                    length = start + index as u64 + 1;
                    self.file.set_len(length)?;
                } else if start == 0 {
                    length = 0;
                    self.file.set_len(0)?;
                } else if let Some((_, retain)) = limits {
                    self.compact(length, retain)?;
                    length = self.file.metadata()?.len();
                } else {
                    return Err(io::Error::other(
                        "unfinished trace record exceeds repair budget",
                    ));
                }
            }
        }
        if let Some((max, retain)) = limits
            && length.saturating_add(line.len() as u64 + 1) > max
        {
            self.compact(length, retain)?;
        }
        if let Some(cap) = cap
            && length.saturating_add(line.len() as u64 + 1)
                > cap.saturating_sub(SESSION_CAP_MARKER.len() as u64)
        {
            return self.mark_capped(length, cap);
        }
        self.file.seek(SeekFrom::End(0))?;
        // One owned buffer preserves the record/newline boundary on successful writes.
        let mut encoded = Vec::with_capacity(line.len() + 1);
        encoded.extend_from_slice(line);
        encoded.push(b'\n');
        self.file.write_all(&encoded)
    }

    /// Record that later records were omitted, once per file (also across
    /// reopened handles), and only while the marker itself fits the cap.
    fn mark_capped(&mut self, length: u64, cap: u64) -> io::Result<()> {
        let marker_len = SESSION_CAP_MARKER.len() as u64;
        if length >= marker_len {
            self.file.seek(SeekFrom::Start(length - marker_len))?;
            let mut tail = vec![0; SESSION_CAP_MARKER.len()];
            self.file.read_exact(&mut tail)?;
            if tail == SESSION_CAP_MARKER {
                return Ok(());
            }
        }
        if length.saturating_add(marker_len) > cap {
            return Ok(());
        }
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(SESSION_CAP_MARKER)
    }

    fn compact(&mut self, length: u64, retain: u64) -> io::Result<()> {
        let start = length.saturating_sub(retain);
        self.file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::new();
        Read::by_ref(&mut self.file)
            .take(retain)
            .read_to_end(&mut tail)?;
        let offset = if start == 0 {
            0
        } else {
            tail.iter()
                .position(|byte| *byte == b'\n')
                .map_or(tail.len(), |index| index + 1)
        };
        let mut output = MARKER.to_vec();
        // An interrupted prior compaction can leave a damaged join. Only retain
        // complete JSON records; the global log is a rolling diagnostic view.
        for line in tail[offset..].split_inclusive(|byte| *byte == b'\n') {
            if line.last() == Some(&b'\n')
                && serde_json::from_slice::<serde_json::Value>(line).is_ok()
            {
                output.extend_from_slice(line);
            }
        }
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&output)?;
        self.file.set_len(output.len() as u64)
    }
}

struct FileLock(File);
impl FileLock {
    fn acquire(file: &File) -> io::Result<Self> {
        let file = file.try_clone()?;
        let deadline = Instant::now() + Duration::from_millis(250);
        loop {
            let result = unsafe {
                LockFileEx(
                    HANDLE(file.as_raw_handle()),
                    LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                    None,
                    u32::MAX,
                    u32::MAX,
                    &mut OVERLAPPED::default(),
                )
            };
            match result {
                Ok(()) => return Ok(Self(file)),
                Err(error) if error.code().0 as u32 == 0x80070021 && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => return Err(io::Error::other(error)),
            }
        }
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = unsafe {
            UnlockFileEx(
                HANDLE(self.0.as_raw_handle()),
                None,
                u32::MAX,
                u32::MAX,
                &mut OVERLAPPED::default(),
            )
        };
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
