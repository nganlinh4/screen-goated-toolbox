//! Stable, strictly bounded reads for the non-mutating text-file capability.

use super::transaction::file_identity;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

#[derive(Debug)]
pub(super) enum ReadFailure {
    Busy,
    ConcurrentChange,
    Missing,
    NotFile,
    Oversize(u64),
    UnsupportedEncoding,
    Io(String),
}

impl ReadFailure {
    pub(super) fn from_io(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::NotFound {
            Self::Missing
        } else {
            Self::Io(error.to_string())
        }
    }

    pub(super) fn into_value(self, path: &Path) -> Value {
        let external_change_detected = matches!(&self, Self::ConcurrentChange);
        let (code, message) = match self {
            Self::Busy => (
                "ERR_TEXT_FILE_BUSY",
                "another writer currently owns the file; no content was returned".to_string(),
            ),
            Self::ConcurrentChange => (
                "ERR_TEXT_FILE_CONCURRENT_CHANGE",
                "the file identity or content changed while it was being read".to_string(),
            ),
            Self::Missing => (
                "ERR_TEXT_FILE_MISSING",
                "the file does not exist".to_string(),
            ),
            Self::NotFile => (
                "ERR_TEXT_FILE_NOT_FILE",
                "the path is not a file".to_string(),
            ),
            Self::Oversize(size) => (
                "ERR_TEXT_FILE_TOO_LARGE",
                format!(
                    "file is {size} bytes; limit is {} bytes",
                    super::MAX_FILE_BYTES
                ),
            ),
            Self::UnsupportedEncoding => (
                "ERR_TEXT_FILE_UNSUPPORTED_ENCODING",
                "only valid UTF-8, with or without a UTF-8 BOM, is supported".to_string(),
            ),
            Self::Io(error) => ("ERR_TEXT_FILE_IO", error),
        };
        let mut value = super::failure(code, Some(path), &message, true);
        if external_change_detected {
            value["external_change_detected"] = Value::Bool(true);
        }
        value
    }
}

pub(super) fn read_stable_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, ReadFailure> {
    // Refuse write/delete sharing. A normal writer or pathname replacement can
    // neither overlap this snapshot nor make bytes and identity disagree.
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)
        .map_err(classify_open_error)?;
    let initial_identity = file_identity(&file).map_err(transaction_error)?;
    let initial = checked_metadata(&file, max_bytes)?;
    let first = read_once(&mut file, max_bytes)?;
    if first.len() as u64 != initial.len() {
        return Err(ReadFailure::ConcurrentChange);
    }

    // A second bounded pass also detects same-identity in-place writes from an
    // already-created mapping, which Windows share modes alone cannot exclude.
    let second = read_once(&mut file, max_bytes)?;
    let final_metadata = checked_metadata(&file, max_bytes)?;
    let final_identity = file_identity(&file).map_err(transaction_error)?;
    if initial_identity != final_identity
        || first != second
        || initial.len() != final_metadata.len()
        || modified_changed(&initial, &final_metadata)
    {
        return Err(ReadFailure::ConcurrentChange);
    }
    Ok(first)
}

fn checked_metadata(file: &File, max_bytes: u64) -> Result<std::fs::Metadata, ReadFailure> {
    let metadata = file
        .metadata()
        .map_err(|error| ReadFailure::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(ReadFailure::NotFile);
    }
    if metadata.len() > max_bytes {
        return Err(ReadFailure::Oversize(metadata.len()));
    }
    Ok(metadata)
}

fn read_once(file: &mut File, max_bytes: u64) -> Result<Vec<u8>, ReadFailure> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| ReadFailure::Io(error.to_string()))?;
    let capacity = usize::try_from(max_bytes.min(64 * 1024)).unwrap_or(64 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| ReadFailure::Io(error.to_string()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(ReadFailure::Oversize(bytes.len() as u64));
    }
    Ok(bytes)
}

fn modified_changed(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    matches!((before.modified(), after.modified()), (Ok(left), Ok(right)) if left != right)
}

fn classify_open_error(error: std::io::Error) -> ReadFailure {
    match error.raw_os_error() {
        Some(32 | 33) => ReadFailure::Busy,
        _ => ReadFailure::from_io(error),
    }
}

fn transaction_error(error: super::transaction::TransactionFailure) -> ReadFailure {
    match error {
        super::transaction::TransactionFailure::Missing => ReadFailure::Missing,
        super::transaction::TransactionFailure::NotFile => ReadFailure::NotFile,
        super::transaction::TransactionFailure::Oversize(size) => ReadFailure::Oversize(size),
        super::transaction::TransactionFailure::Busy(_) => ReadFailure::Busy,
        super::transaction::TransactionFailure::Io(message) => ReadFailure::Io(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_WRITE};

    fn fixture(bytes: &[u8]) -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "sgt-stable-read-{}-{}.txt",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn bounded_primitive_never_reads_past_limit() {
        let path = fixture(b"abcde");
        let mut file = File::open(&path).unwrap();
        assert!(matches!(
            read_once(&mut file, 4),
            Err(ReadFailure::Oversize(5))
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn live_writer_fails_closed_instead_of_returning_a_torn_snapshot() {
        let path = fixture(b"stable");
        let writer = OpenOptions::new()
            .write(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
            .open(&path)
            .unwrap();
        assert!(matches!(
            read_stable_bounded(&path, 64),
            Err(ReadFailure::Busy)
        ));
        drop(writer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stable_read_returns_the_exact_snapshot() {
        let path = fixture("alpha-βeta".as_bytes());
        assert_eq!(
            read_stable_bounded(&path, 64).unwrap(),
            "alpha-βeta".as_bytes()
        );
        fs::remove_file(path).unwrap();
    }
}
