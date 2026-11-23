use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use anyhow::Result;

use super::catalog::validate_identifier;

#[derive(Default)]
struct LeaseState {
    counts: HashMap<String, usize>,
    pending_removals: std::collections::HashSet<String>,
    removals_in_progress: std::collections::HashSet<String>,
    removal_locks: HashMap<String, super::file_lock::FileLease>,
}

static LEASES: LazyLock<Mutex<LeaseState>> = LazyLock::new(|| Mutex::new(LeaseState::default()));
#[derive(Default)]
struct WatchState {
    entries: HashMap<String, RetryState>,
    running: bool,
}

struct RetryState {
    next: std::time::Instant,
    delay: std::time::Duration,
}

static WATCHERS: LazyLock<Mutex<WatchState>> = LazyLock::new(|| Mutex::new(WatchState::default()));

pub(crate) struct ComponentLease {
    id: String,
    held: bool,
    lock: Option<super::file_lock::FileLease>,
}

pub(super) struct ExclusiveComponentMutation {
    id: String,
    _lock: super::file_lock::FileLease,
}

impl ExclusiveComponentMutation {
    pub(super) fn matches(&self, id: &str) -> bool {
        self.id == id
    }
}

pub(crate) fn acquire(id: &str) -> Result<ComponentLease> {
    validate_identifier(id)?;
    let _mutation = super::acquire_mutation_guard()?;
    if super::pending::list()?.iter().any(|pending| pending == id) {
        anyhow::bail!("component removal is pending");
    }
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    if state.pending_removals.contains(id) || state.removals_in_progress.contains(id) {
        anyhow::bail!("component removal is pending");
    }
    let lock = super::file_lock::FileLease::shared(id)?;
    *state.counts.entry(id.to_string()).or_default() += 1;
    Ok(ComponentLease {
        id: id.to_string(),
        held: true,
        lock: Some(lock),
    })
}

impl Drop for ComponentLease {
    fn drop(&mut self) {
        if !self.held {
            return;
        }
        drop(self.lock.take());
        let last_local_lease = {
            let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
            let Some(count) = state.counts.get_mut(&self.id) else {
                return;
            };
            *count -= 1;
            if *count != 0 {
                false
            } else {
                state.counts.remove(&self.id);
                true
            }
        };
        if last_local_lease {
            start_pending_watcher(&self.id);
        }
    }
}

pub(super) fn upgrade_to_exclusive(
    lease: &mut ComponentLease,
) -> Result<ExclusiveComponentMutation> {
    let _mutation = super::acquire_mutation_guard()?;
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    if !lease.held
        || state.pending_removals.contains(&lease.id)
        || state.removals_in_progress.contains(&lease.id)
        || state.counts.get(&lease.id).copied() != Some(1)
    {
        anyhow::bail!("component is currently in use or pending removal");
    }
    let lock = lease
        .lock
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("component lease is unavailable"))?;
    if !lock.upgrade()? {
        anyhow::bail!("component is currently in use in another process");
    }
    let lock = lease.lock.take().expect("lease lock was checked");
    state.counts.remove(&lease.id);
    state.removals_in_progress.insert(lease.id.clone());
    lease.held = false;
    Ok(ExclusiveComponentMutation {
        id: lease.id.clone(),
        _lock: lock,
    })
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn reserve_exclusive_mutation(id: &str) -> Result<ExclusiveComponentMutation> {
    validate_identifier(id)?;
    let _mutation = super::acquire_mutation_guard()?;
    if super::pending::list()?.iter().any(|pending| pending == id) {
        anyhow::bail!("component removal is pending");
    }
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    if state.pending_removals.contains(id)
        || state.removals_in_progress.contains(id)
        || state.counts.get(id).copied().unwrap_or_default() != 0
    {
        anyhow::bail!("component is currently in use or pending removal");
    }
    let lock = super::file_lock::FileLease::exclusive(id)?
        .ok_or_else(|| anyhow::anyhow!("component is currently in use in another process"))?;
    state.removals_in_progress.insert(id.to_string());
    Ok(ExclusiveComponentMutation {
        id: id.to_string(),
        _lock: lock,
    })
}

impl Drop for ExclusiveComponentMutation {
    fn drop(&mut self) {
        LEASES
            .lock()
            .unwrap_or_else(|value| value.into_inner())
            .removals_in_progress
            .remove(&self.id);
    }
}

pub(super) fn reserve_removal(id: &str) -> Result<bool> {
    let _mutation = super::acquire_mutation_guard()?;
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    state.pending_removals.insert(id.to_string());
    if state.counts.get(id).copied().unwrap_or_default() != 0
        || state.removals_in_progress.contains(id)
    {
        drop(state);
        start_pending_watcher(id);
        Ok(true)
    } else {
        match super::file_lock::FileLease::exclusive(id)? {
            Some(lock) => {
                state.removals_in_progress.insert(id.to_string());
                state.removal_locks.insert(id.to_string(), lock);
                Ok(false)
            }
            None => {
                drop(state);
                start_pending_watcher(id);
                Ok(true)
            }
        }
    }
}

pub(super) fn finish_removal(id: &str, clear_pending: bool) {
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    state.removals_in_progress.remove(id);
    state.removal_locks.remove(id);
    if clear_pending {
        state.pending_removals.remove(id);
    }
}

pub(super) fn start_pending_watcher(id: &str) {
    let minimum = std::time::Duration::from_millis(200);
    let mut state = WATCHERS.lock().unwrap_or_else(|value| value.into_inner());
    state
        .entries
        .entry(id.to_string())
        .or_insert_with(|| RetryState {
            next: std::time::Instant::now() + minimum,
            delay: minimum,
        });
    if state.running {
        return;
    }
    state.running = true;
    drop(state);
    std::thread::spawn(watch_pending_removals);
}

pub(super) fn start_pending_scan() {
    start_pending_watcher("");
}

fn watch_pending_removals() {
    let minimum = std::time::Duration::from_millis(200);
    let maximum = std::time::Duration::from_secs(30);
    loop {
        let ready = {
            let mut state = WATCHERS.lock().unwrap_or_else(|value| value.into_inner());
            if state.entries.is_empty() {
                state.running = false;
                return;
            }
            let now = std::time::Instant::now();
            state
                .entries
                .iter()
                .filter_map(|(id, retry)| (retry.next <= now).then_some(id.clone()))
                .collect::<Vec<_>>()
        };
        if ready.is_empty() {
            std::thread::sleep(minimum);
            continue;
        }
        for id in ready {
            let attempt = (|| -> Result<Option<super::RemovalOutcome>> {
                let _mutation = super::acquire_mutation_guard()?;
                let pending = super::pending::list()?;
                if id.is_empty() {
                    for pending_id in pending {
                        start_pending_watcher(&pending_id);
                    }
                    WATCHERS
                        .lock()
                        .unwrap_or_else(|value| value.into_inner())
                        .entries
                        .remove(&id);
                    return Ok(None);
                }
                if !pending.contains(&id) {
                    let mut leases = LEASES.lock().unwrap_or_else(|value| value.into_inner());
                    if !leases.removals_in_progress.contains(&id) {
                        leases.pending_removals.remove(&id);
                    }
                    WATCHERS
                        .lock()
                        .unwrap_or_else(|value| value.into_inner())
                        .entries
                        .remove(&id);
                    return Ok(None);
                }
                super::removal::retry_pending_removal(&id)
            })();
            let mut state = WATCHERS.lock().unwrap_or_else(|value| value.into_inner());
            let Some(retry) = state.entries.get_mut(&id) else {
                continue;
            };
            retry.delay = match attempt {
                Ok(None) => minimum,
                Ok(Some(super::RemovalOutcome::RequiredBy(_))) => std::time::Duration::from_secs(1),
                Ok(Some(_)) => minimum,
                Err(error) => {
                    if id.is_empty() {
                        crate::log_info!("[Components] pending removal scan will retry: {error:#}");
                    } else {
                        crate::log_info!(
                            "[Components] pending removal of {id} will retry: {error:#}"
                        );
                    }
                    retry.delay.saturating_mul(2).min(maximum)
                }
            };
            retry.next = std::time::Instant::now() + retry.delay;
        }
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn watcher_active(id: &str) -> bool {
    WATCHERS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .entries
        .contains_key(id)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn pending(id: &str) -> bool {
    LEASES
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .pending_removals
        .contains(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_reservation_rejects_new_leases_until_finished() {
        let id = "test-removal-reservation";
        assert!(!reserve_removal(id).unwrap());
        assert!(acquire(id).is_err());
        finish_removal(id, true);
        assert!(acquire(id).is_ok());
    }

    #[test]
    #[cfg(not(feature = "recorder-worker"))]
    fn exclusive_mutation_requires_zero_leases_and_blocks_new_ones() {
        let id = "test-exclusive-mutation";
        let lease = acquire(id).unwrap();
        assert!(reserve_exclusive_mutation(id).is_err());
        drop(lease);
        let mutation = reserve_exclusive_mutation(id).unwrap();
        assert!(acquire(id).is_err());
        drop(mutation);
        assert!(acquire(id).is_ok());
    }

    #[test]
    fn upgrade_fails_with_other_users_and_excludes_new_leases() {
        let id = "test-lease-upgrade";
        let mut installer = acquire(id).unwrap();
        let user = acquire(id).unwrap();
        assert!(upgrade_to_exclusive(&mut installer).is_err());
        drop(user);
        let exclusive = upgrade_to_exclusive(&mut installer).unwrap();
        assert!(exclusive.matches(id));
        assert!(acquire(id).is_err());
        drop(installer);
        assert!(acquire(id).is_err());
        drop(exclusive);
        assert!(acquire(id).is_ok());
    }

    #[test]
    fn final_lease_drop_does_not_wait_for_registry_mutex() {
        let guard = super::super::acquire_mutation_guard().unwrap();
        let lease = acquire("test-nonblocking-lease-drop").unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            drop(lease);
            sender.send(()).unwrap();
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .is_ok()
        );
        drop(guard);
        thread.join().unwrap();
    }
}
