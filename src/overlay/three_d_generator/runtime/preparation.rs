use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::Duration;

use serde::Deserialize;

use super::process::CommandNoWindowExt as _;
use super::runtime_command;

const SESSION_TTL_MS: u64 = 2 * 60 * 60 * 1000;
const SESSION_REFRESH_AHEAD_MS: u64 = 20 * 60 * 1000;
const SESSION_TARGET: usize = 4;
const MAX_PARALLEL_WARMERS: usize = 1;
const MAINTENANCE_IDLE_INTERVAL: Duration = Duration::from_secs(60);
const MAINTENANCE_BATCH_GAP: Duration = Duration::from_secs(15);
const MAINTENANCE_REFRESH_GAP: Duration = Duration::from_secs(5 * 60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreparedSessionMarker {
    profile_dir: String,
    created_at: u64,
}

static WARM_RUNNING: AtomicBool = AtomicBool::new(false);
static MAINTAINER_STARTED: AtomicBool = AtomicBool::new(false);
static INSTALL_ALLOWED: AtomicBool = AtomicBool::new(false);
static MAINTENANCE_SIGNAL: LazyLock<(Mutex<bool>, Condvar)> =
    LazyLock::new(|| (Mutex::new(false), Condvar::new()));

fn prepared_sessions_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("prepared-sessions")
}

fn prepared_ready_paths() -> Vec<PathBuf> {
    let dir = prepared_sessions_dir();
    (0..SESSION_TARGET)
        .map(|slot| dir.join(format!("ready-{slot}.json")))
        .collect()
}

fn prepared_lock_paths() -> Vec<PathBuf> {
    let dir = prepared_sessions_dir();
    (0..SESSION_TARGET)
        .map(|slot| dir.join(format!("warming-{slot}.lock")))
        .collect()
}

pub(super) fn runtime_preparation_status() -> String {
    if prepared_session_count() >= SESSION_TARGET {
        "ready".to_string()
    } else if WARM_RUNNING.load(Ordering::SeqCst)
        || prepared_lock_is_active()
        || MAINTAINER_STARTED.load(Ordering::SeqCst)
    {
        "preparing".to_string()
    } else if runtime_command().is_none() {
        "missing".to_string()
    } else {
        "not_ready".to_string()
    }
}

fn prepared_lock_is_active() -> bool {
    prepared_lock_paths().into_iter().any(|lock_path| {
        let age = std::fs::metadata(&lock_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.elapsed().ok());
        if age.is_some_and(|value| value <= Duration::from_secs(3 * 60)) {
            return true;
        }
        if lock_path.exists() {
            let _ = std::fs::remove_file(lock_path);
        }
        false
    })
}

fn prepared_marker(ready_path: &Path) -> Option<PreparedSessionMarker> {
    std::fs::read_to_string(ready_path)
        .ok()
        .and_then(|contents| serde_json::from_str::<PreparedSessionMarker>(&contents).ok())
}

fn prepared_marker_age_ms(marker: &PreparedSessionMarker, now_ms: u64) -> Option<u64> {
    (!marker.profile_dir.trim().is_empty()
        && Path::new(&marker.profile_dir).is_dir()
        && marker.created_at <= now_ms)
        .then(|| now_ms.saturating_sub(marker.created_at))
}

fn prepared_marker_is_valid(ready_path: &Path) -> bool {
    let marker = prepared_marker(ready_path);
    let now_ms = now_ms();
    marker
        .as_ref()
        .and_then(|marker| prepared_marker_age_ms(marker, now_ms))
        .is_some_and(|age_ms| age_ms <= SESSION_TTL_MS)
}

fn prepared_marker_is_healthy(ready_path: &Path) -> bool {
    let marker = prepared_marker(ready_path);
    let now_ms = now_ms();
    marker
        .as_ref()
        .and_then(|marker| prepared_marker_age_ms(marker, now_ms))
        .is_some_and(|age_ms| age_ms <= SESSION_TTL_MS.saturating_sub(SESSION_REFRESH_AHEAD_MS))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn prepared_session_count() -> usize {
    prepared_ready_paths()
        .into_iter()
        .filter(|path| prepared_marker_is_valid(path))
        .count()
}

fn healthy_prepared_session_count() -> usize {
    prepared_ready_paths()
        .into_iter()
        .filter(|path| prepared_marker_is_healthy(path))
        .count()
}

fn warm_batch_size(valid_count: usize, healthy_count: usize) -> usize {
    let needed = SESSION_TARGET.saturating_sub(healthy_count);
    if needed == 0 {
        0
    } else if valid_count >= SESSION_TARGET {
        1
    } else {
        needed.min(MAX_PARALLEL_WARMERS)
    }
}

fn retry_delay(failure_streak: u32) -> Duration {
    Duration::from_secs((failure_streak.max(1) as u64 * 5 * 60).min(15 * 60))
}

fn wake_maintainer() {
    let (pending, signal) = &*MAINTENANCE_SIGNAL;
    if let Ok(mut pending) = pending.lock() {
        *pending = true;
        signal.notify_one();
    }
}

fn wait_for_maintenance(delay: Duration) {
    let (pending, signal) = &*MAINTENANCE_SIGNAL;
    let Ok(mut pending) = pending.lock() else {
        std::thread::sleep(delay);
        return;
    };
    if !*pending {
        let Ok((next, _)) = signal.wait_timeout(pending, delay) else {
            return;
        };
        pending = next;
    }
    *pending = false;
}

fn run_warm_batch(count: usize) -> (usize, usize) {
    WARM_RUNNING.store(true, Ordering::SeqCst);
    let mut warmers = Vec::with_capacity(count);
    for _ in 0..count {
        warmers.push(std::thread::spawn(|| {
            let Some(mut command) = runtime_command() else {
                return false;
            };
            command
                .arg("--warm-headless")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .creation_flags_windows();
            command.status().is_ok_and(|status| status.success())
        }));
    }
    let successes = warmers
        .into_iter()
        .map(|warmer| warmer.join().unwrap_or(false))
        .filter(|success| *success)
        .count();
    WARM_RUNNING.store(false, Ordering::SeqCst);
    (successes, count.saturating_sub(successes))
}

fn preparation_maintainer() {
    let mut failure_streak = 0_u32;
    loop {
        if runtime_command().is_none() {
            if !INSTALL_ALLOWED.load(Ordering::SeqCst) {
                wait_for_maintenance(MAINTENANCE_IDLE_INTERVAL);
                continue;
            }
            WARM_RUNNING.store(true, Ordering::SeqCst);
            let stop = std::sync::Arc::new(AtomicBool::new(false));
            let installed = crate::overlay::creation_runtime::download_runtime(stop, true);
            WARM_RUNNING.store(false, Ordering::SeqCst);
            if let Err(error) = installed {
                failure_streak = failure_streak.saturating_add(1);
                let delay = retry_delay(failure_streak);
                crate::log_info!(
                    "[3D Generator] Native engine install failed; retrying in {}s: {error}",
                    delay.as_secs()
                );
                std::thread::sleep(delay);
                continue;
            }
            failure_streak = 0;
        }

        let valid_count = prepared_session_count();
        let healthy_count = healthy_prepared_session_count();
        let batch_size = warm_batch_size(valid_count, healthy_count);
        if batch_size == 0 {
            failure_streak = 0;
            wait_for_maintenance(MAINTENANCE_IDLE_INTERVAL);
            continue;
        }

        let (successes, failures) = run_warm_batch(batch_size);
        if failures > 0 {
            failure_streak = failure_streak.saturating_add(1);
            let delay = retry_delay(failure_streak);
            crate::log_info!(
                "[3D Generator] {failures} preparation worker(s) failed; retrying in {}s",
                delay.as_secs()
            );
            std::thread::sleep(delay);
        } else {
            failure_streak = 0;
            let delay = if successes > 0 && healthy_prepared_session_count() < SESSION_TARGET {
                if prepared_session_count() >= SESSION_TARGET {
                    MAINTENANCE_REFRESH_GAP
                } else {
                    MAINTENANCE_BATCH_GAP
                }
            } else {
                MAINTENANCE_IDLE_INTERVAL
            };
            wait_for_maintenance(delay);
        }
    }
}

pub(super) fn start_preparation_maintainer(install_if_missing: bool) {
    if install_if_missing {
        INSTALL_ALLOWED.store(true, Ordering::SeqCst);
    } else if runtime_command().is_none() {
        return;
    }
    if MAINTAINER_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::thread::spawn(preparation_maintainer);
    }
    wake_maintainer();
}

pub(super) fn prepare_runtime() -> String {
    start_preparation_maintainer(true);
    runtime_preparation_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_backed_warmups_are_serial_but_keep_four_ready_slots() {
        assert_eq!(warm_batch_size(0, 0), 1);
        assert_eq!(warm_batch_size(3, 3), 1);
        assert_eq!(warm_batch_size(4, 3), 1);
        assert_eq!(warm_batch_size(4, 4), 0);
    }

    #[test]
    fn retry_delay_starts_at_five_minutes_and_caps_at_fifteen() {
        assert_eq!(retry_delay(1), Duration::from_secs(300));
        assert_eq!(retry_delay(2), Duration::from_secs(600));
        assert_eq!(retry_delay(3), Duration::from_secs(900));
        assert_eq!(retry_delay(20), Duration::from_secs(900));
    }
}
