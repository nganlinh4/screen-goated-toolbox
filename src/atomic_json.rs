//! Atomic JSON file writes.
//!
//! The whole application config (every preset, preset profile, all API keys,
//! all TTS/realtime settings) and the history database each live in a single
//! JSON file. Writing them with a plain truncate-in-place (`fs::write` /
//! `File::create`) means a crash or power loss mid-write leaves a truncated,
//! unparseable file — which the loader then silently resets to defaults,
//! permanently wiping the user's data.
//!
//! [`write_json_atomic`] avoids that: it serializes to a unique sibling file,
//! fsyncs it, then `rename`s over the destination. `rename` is atomic on the
//! same volume (NTFS included), so a reader either sees the complete old file
//! or the complete new one — never a partial write. On any error the original
//! file at `path` is left untouched.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    for _ in 0..128 {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(
            ".tmp-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let tmp_path = PathBuf::from(name);
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let tmp = TemporaryFile(tmp_path);
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        return std::fs::rename(&tmp.0, path);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a temporary sibling",
    ))
}

/// Write raw bytes to `path` atomically (temp file + fsync + rename). Leaves the
/// existing file untouched on failure, so a partial write never replaces a good
/// file. Used for payloads that must stay byte-exact because a signature is
/// computed over them.
#[cfg(not(feature = "recorder-worker"))]
pub fn write_bytes_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    write_atomic(path, data)
}

/// Serialize `value` as pretty JSON and write it to `path` atomically
/// (temp file + fsync + rename). Leaves the existing file untouched on failure.
pub fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let data = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    write_atomic(path, &data)
}

/// Runs `replace` while `path` is read-only, so a test can check that a failed
/// replacement leaves the existing file untouched. Restores the attribute.
/// Only the app's GUI writers use it; the recorder worker shares this file.
#[cfg(all(test, not(feature = "recorder-worker")))]
pub(crate) fn while_read_only<T>(path: &Path, replace: impl FnOnce() -> T) -> T {
    let original = std::fs::metadata(path).unwrap().permissions();
    let mut read_only = original.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(path, read_only).unwrap();
    let result = replace();
    std::fs::set_permissions(path, original).unwrap();
    result
}

#[cfg(test)]
mod tests {
    use super::write_json_atomic;
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_writes_publish_complete_documents_and_remove_temporary_files() {
        let root = std::env::temp_dir().join(format!(
            "sgt-atomic-json-{}-{}",
            std::process::id(),
            super::TEMP_SEQUENCE.fetch_add(1, super::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("settings.json");
        let barrier = Arc::new(Barrier::new(4));
        std::thread::scope(|scope| {
            for index in 0..4 {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    for revision in 0..8 {
                        write_json_atomic(
                            &path,
                            &serde_json::json!({
                                "writer": index,
                                "revision": revision,
                                "payload": "x".repeat(4096),
                            }),
                        )
                        .unwrap();
                    }
                });
            }
        });
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["payload"].as_str().unwrap().len(), 4096);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
