use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Result, bail};
use sha2::{Digest as _, Sha256};

use super::{MAX_VERIFICATION_WORKERS, RecorderFile};
use crate::component_registry::receipt::{is_reparse_point, resolve_owned_path};

/// Locks and verifies the inventory across a small pool. Every file is still
/// opened, size-checked, and fully hashed before its handle is retained; the
/// pool only overlaps the per-file open latency, which dominates a cold start.
pub(super) fn lock_component_files(
    root: &Path,
    files: &[RecorderFile],
) -> Result<Vec<std::fs::File>> {
    let workers = verification_workers(files.len());
    if workers <= 1 {
        return files
            .iter()
            .map(|expected| lock_verified_file(root, expected))
            .collect();
    }

    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut locked = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(expected) = files.get(index) else {
                            return Ok(locked);
                        };
                        locked.push(lock_verified_file(root, expected)?);
                    }
                })
            })
            .collect();

        let mut locked = Vec::with_capacity(files.len());
        let mut failure = None;
        for handle in handles {
            match handle.join() {
                Ok(Ok(mut files)) => locked.append(&mut files),
                Ok(Err(error)) => failure = failure.or(Some(error)),
                Err(_) => {
                    failure = failure
                        .or_else(|| Some(anyhow::anyhow!("recorder verification worker panicked")))
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(locked),
        }
    })
}

pub(super) fn verification_workers(files: usize) -> usize {
    let available = std::thread::available_parallelism().map_or(1, |value| value.get());
    files.clamp(1, available.min(MAX_VERIFICATION_WORKERS))
}

fn lock_verified_file(root: &Path, expected: &RecorderFile) -> Result<std::fs::File> {
    let path = resolve_owned_path(root, Path::new(expected.path))?;
    let mut file = open_locked_regular_file(&path)?;
    if file.metadata()?.len() != expected.size_bytes {
        bail!("recorder component changed while acquiring its launch lease");
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected.sha256) {
        bail!("recorder component changed while acquiring its launch lease");
    }
    Ok(file)
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("recorder launch file is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    options.share_mode(FILE_SHARE_READ.0);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("recorder launch file is unsafe");
    }
    Ok(file)
}

pub(super) fn validate_x64_pe(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("recorder worker is not a PE executable");
    }
    let offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap());
    file.seek(SeekFrom::Start(offset as u64))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    if &header[..4] != b"PE\0\0" || u16::from_le_bytes([header[4], header[5]]) != 0x8664 {
        bail!("recorder worker is not an x64 PE executable");
    }
    Ok(())
}
