//! Atomic JSON file writes.
//!
//! The whole application config (every preset, preset profile, all API keys,
//! all TTS/realtime settings) and the history database each live in a single
//! JSON file. Writing them with a plain truncate-in-place (`fs::write` /
//! `File::create`) means a crash or power loss mid-write leaves a truncated,
//! unparseable file — which the loader then silently resets to defaults,
//! permanently wiping the user's data.
//!
//! [`write_json_atomic`] avoids that: it serializes to a sibling `<path>.tmp`,
//! fsyncs it, then `rename`s over the destination. `rename` is atomic on the
//! same volume (NTFS included), so a reader either sees the complete old file
//! or the complete new one — never a partial write. On any error the original
//! file at `path` is left untouched.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Serialize `value` as pretty JSON and write it to `path` atomically
/// (temp file + fsync + rename). Leaves the existing file untouched on failure.
pub fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let data = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&data)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}
