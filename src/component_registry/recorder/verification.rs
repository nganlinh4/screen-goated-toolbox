pub(super) use super::super::native_file::validate_x64_pe;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;

use super::{MAX_VERIFICATION_WORKERS, RecorderFile};

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
    crate::component_registry::native_file::lock_verified(
        root,
        expected.path,
        expected.size_bytes,
        expected.sha256,
        "recorder component changed while acquiring its launch lease",
    )
}
