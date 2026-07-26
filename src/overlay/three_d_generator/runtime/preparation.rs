use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::Duration;

use super::process::CommandNoWindowExt as _;
use super::runtime_command;

const MAX_PARALLEL_WARMERS: usize = 1;
const MAINTENANCE_IDLE_INTERVAL: Duration = Duration::from_secs(60);
const MAINTENANCE_BATCH_GAP: Duration = Duration::from_secs(15);

static WARM_RUNNING: AtomicBool = AtomicBool::new(false);
static MAINTAINER_STARTED: AtomicBool = AtomicBool::new(false);
static INSTALL_ALLOWED: AtomicBool = AtomicBool::new(false);
static MAINTENANCE_SIGNAL: LazyLock<(Mutex<bool>, Condvar)> =
    LazyLock::new(|| (Mutex::new(false), Condvar::new()));
static PREPARATION_STATUS: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new("not_ready".to_string()));

pub(super) fn runtime_preparation_status() -> String {
    if runtime_command().is_none() {
        return "missing".to_string();
    }
    let status = PREPARATION_STATUS
        .lock()
        .map(|value| value.clone())
        .unwrap_or_else(|value| value.into_inner().clone());
    if status == "ready" {
        status
    } else if WARM_RUNNING.load(Ordering::SeqCst) || MAINTAINER_STARTED.load(Ordering::SeqCst) {
        "preparing".to_string()
    } else {
        status
    }
}

fn refresh_preparation_status() -> bool {
    let Some(status) = crate::overlay::creation_runtime::query_preparation_status("3d") else {
        return true;
    };
    *PREPARATION_STATUS
        .lock()
        .unwrap_or_else(|value| value.into_inner()) = status.state;
    status.needs_preparation
}

fn warm_batch_size(needs_preparation: bool) -> usize {
    usize::from(needs_preparation).min(MAX_PARALLEL_WARMERS)
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

        let needs_preparation = refresh_preparation_status();
        let batch_size = warm_batch_size(needs_preparation);
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
            let delay = if successes > 0 && refresh_preparation_status() {
                MAINTENANCE_BATCH_GAP
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
    fn warmups_are_serial_and_stop_when_preparation_is_complete() {
        assert_eq!(warm_batch_size(true), 1);
        assert_eq!(warm_batch_size(false), 0);
    }

    #[test]
    fn retry_delay_starts_at_five_minutes_and_caps_at_fifteen() {
        assert_eq!(retry_delay(1), Duration::from_secs(300));
        assert_eq!(retry_delay(2), Duration::from_secs(600));
        assert_eq!(retry_delay(3), Duration::from_secs(900));
        assert_eq!(retry_delay(20), Duration::from_secs(900));
    }
}
