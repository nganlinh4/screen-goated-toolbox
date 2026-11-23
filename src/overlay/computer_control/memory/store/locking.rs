//! A bounded cross-process transaction lock scoped to the exact store directory.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Context;

pub(super) const LOCK_FILE: &str = "store.lock";

pub(super) fn acquire(dir: &Path) -> anyhow::Result<File> {
    std::fs::create_dir_all(dir)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(LOCK_FILE))
        .context("open conversation memory lock")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => anyhow::bail!("conversation memory lock unavailable: {error}"),
        }
    }
}
