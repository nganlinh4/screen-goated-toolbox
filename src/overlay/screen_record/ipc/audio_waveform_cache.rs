//! Admission bound for cache files; existing entries are never deleted here.
use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Result, bail};
use windows::Win32::Storage::FileSystem::{DELETE, FILE_FLAG_DELETE_ON_CLOSE, FILE_GENERIC_WRITE};

const MAX_FILES: usize = 512;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Hold the exclusive lease through publication so cooperating writers cannot
/// simultaneously spend the same remaining capacity. Failure is a cache miss,
/// not a failure of waveform analysis.
pub fn admit(root: &Path, incoming_bytes: u64) -> Result<File> {
    let lock_path = root.join("waveform-admission.lock");
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(0)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0)
        .open(&lock_path)?;
    let mut files = 0_usize;
    let mut bytes = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.path() == lock_path {
            continue;
        }
        files = files
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("waveform cache entry count overflow"))?;
        bytes = bytes
            .checked_add(fs::symlink_metadata(entry.path())?.len())
            .ok_or_else(|| anyhow::anyhow!("waveform cache size overflow"))?;
        if files >= MAX_FILES || bytes.saturating_add(incoming_bytes) > MAX_BYTES {
            bail!("waveform cache capacity reached");
        }
    }
    if incoming_bytes > MAX_BYTES {
        bail!("waveform cache capacity reached");
    }
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_preserves_unowned_entries_and_serializes_admission() {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "sgt-waveform-capacity-{:016x}",
            u64::from_le_bytes(nonce)
        ));
        fs::create_dir(&root).unwrap();
        let sentinel = root.join("unowned.bin");
        fs::write(&sentinel, b"keep").unwrap();
        let lease = admit(&root, 1).unwrap();
        assert!(admit(&root, 1).is_err());
        drop(lease);
        for index in 1..MAX_FILES {
            fs::write(root.join(format!("{index:04}.bin")), b"data").unwrap();
        }
        assert!(admit(&root, 1).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
        assert_eq!(fs::read_dir(&root).unwrap().count(), MAX_FILES);
        fs::remove_dir_all(root).unwrap();
    }
}
