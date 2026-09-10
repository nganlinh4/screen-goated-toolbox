use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, TryLockError, Weak};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use sha2::Digest as _;

use super::{FileContract, verified_file_present};

#[path = "verified_download/scratch.rs"]
mod scratch;

type DownloadLocks = HashMap<PathBuf, Weak<Mutex<()>>>;
static DOWNLOAD_LOCKS: LazyLock<Mutex<DownloadLocks>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn download_lock(path: &Path) -> Arc<Mutex<()>> {
    let mut locks = DOWNLOAD_LOCKS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn wait_for_download<'a>(
    lock: &'a Mutex<()>,
    stop_signal: &AtomicBool,
) -> Result<MutexGuard<'a, ()>> {
    loop {
        check_cancelled(stop_signal)?;
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::Poisoned(error)) => return Ok(error.into_inner()),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(40)),
        }
    }
}

/// File delivery owns byte verification; callers own aggregate progress and UI.
pub(crate) fn download_verified_file_with_progress(
    contract: FileContract,
    url: &str,
    path: &Path,
    stop_signal: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let started = Instant::now();
    let mut downloaded = 0_u64;
    let result = (|| -> Result<()> {
        let lock = download_lock(path);
        let _guard = wait_for_download(&lock, stop_signal)?;
        check_cancelled(stop_signal)?;
        if verified_file_present(path, contract) {
            on_progress(contract.size_bytes, contract.size_bytes);
            crate::log_info!(
                "[ModelDownload] file={} phase=verified_existing",
                contract.name
            );
            return Ok(());
        }
        check_cancelled(stop_signal)?;
        let parent = path
            .parent()
            .context("model file has no parent directory")?;
        fs::create_dir_all(parent).context("create model download directory")?;
        // Unique create-new staging never removes another transfer's or an unowned partial file.
        crate::log_info!(
            "[ModelDownload] file={} phase=request expected_bytes={}",
            contract.name,
            contract.size_bytes
        );
        on_progress(0, contract.size_bytes);
        check_cancelled(stop_signal)?;
        let response = crate::api::client::UREQ_DOWNLOAD_AGENT
            .get(url)
            .header("User-Agent", "ScreenGoatedToolbox")
            .call()
            .context("request model download")?;
        let announced_size = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if let Some(size) = announced_size
            && size != contract.size_bytes
        {
            bail!(
                "model response size mismatch: expected {}, received {size}",
                contract.size_bytes
            );
        }
        check_cancelled(stop_signal)?;
        let mut reader = response.into_body().into_reader();
        let mut scratch = scratch::DownloadScratch::create(path)?;
        let output = &mut scratch.file;
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        let mut last_update = Instant::now();
        let mut last_log = Instant::now();
        loop {
            check_cancelled(stop_signal)?;
            let read = reader
                .read(&mut buffer)
                .context("read model download body")?;
            if read == 0 {
                break;
            }
            downloaded = downloaded
                .checked_add(read as u64)
                .filter(|bytes| *bytes <= contract.size_bytes)
                .context("model download exceeds its declared size")?;
            output
                .write_all(&buffer[..read])
                .context("write model download")?;
            hasher.update(&buffer[..read]);
            if last_update.elapsed() >= Duration::from_millis(100) {
                on_progress(downloaded, contract.size_bytes);
                last_update = Instant::now();
            }
            if last_log.elapsed() >= Duration::from_secs(5) {
                crate::log_info!(
                    "[ModelDownload] file={} phase=receiving bytes={}/{} elapsed_ms={}",
                    contract.name,
                    downloaded,
                    contract.size_bytes,
                    started.elapsed().as_millis()
                );
                last_log = Instant::now();
            }
        }
        output.flush().context("flush model download")?;
        output.sync_all().context("sync model download")?;
        let digest = format!("{:x}", hasher.finalize());
        if downloaded != contract.size_bytes || !digest.eq_ignore_ascii_case(contract.sha256) {
            bail!(
                "model integrity mismatch: bytes={downloaded}/{} sha256={digest} expected_sha256={}",
                contract.size_bytes,
                contract.sha256
            );
        }
        check_cancelled(stop_signal)?;
        scratch.publish(path)?;
        on_progress(contract.size_bytes, contract.size_bytes);
        crate::log_info!(
            "[ModelDownload] file={} phase=verified bytes={} elapsed_ms={}",
            contract.name,
            downloaded,
            started.elapsed().as_millis()
        );
        Ok(())
    })();
    if let Err(error) = &result {
        crate::log_info!(
            "[ModelDownload] file={} phase=ended cancelled={} bytes={}/{} elapsed_ms={} error={error:#}",
            contract.name,
            stop_signal.load(Ordering::Relaxed),
            downloaded,
            contract.size_bytes,
            started.elapsed().as_millis()
        );
    }
    result.map_err(|error| anyhow::anyhow!("Download failed for {}: {error:#}", contract.name))
}

fn check_cancelled(stop_signal: &AtomicBool) -> Result<()> {
    if stop_signal.load(Ordering::Relaxed) {
        bail!("Download cancelled");
    }
    Ok(())
}

#[cfg(test)]
#[path = "verified_download/tests.rs"]
mod tests;
