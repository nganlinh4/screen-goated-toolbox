use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use anyhow::Result;

use super::catalog::validate_identifier;

#[derive(Default)]
struct LeaseState {
    counts: HashMap<String, usize>,
    pending_removals: std::collections::HashSet<String>,
    removals_in_progress: std::collections::HashSet<String>,
}

static LEASES: LazyLock<Mutex<LeaseState>> = LazyLock::new(|| Mutex::new(LeaseState::default()));

pub(crate) struct ComponentLease {
    id: String,
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) struct ExclusiveComponentMutation {
    id: String,
}

pub(crate) fn acquire(id: &str) -> Result<ComponentLease> {
    validate_identifier(id)?;
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    if state.pending_removals.contains(id) || state.removals_in_progress.contains(id) {
        anyhow::bail!("component removal is pending");
    }
    *state.counts.entry(id.to_string()).or_default() += 1;
    Ok(ComponentLease { id: id.to_string() })
}

impl Drop for ComponentLease {
    fn drop(&mut self) {
        let owns_removal = {
            let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
            let Some(count) = state.counts.get_mut(&self.id) else {
                return;
            };
            *count -= 1;
            if *count != 0 {
                false
            } else {
                state.counts.remove(&self.id);
                let pending = state.pending_removals.contains(&self.id);
                if pending {
                    state.removals_in_progress.insert(self.id.clone());
                }
                pending
            }
        };
        if owns_removal
            && let Ok(outcome) = super::removal::run_reserved_removal(&self.id)
            && matches!(
                outcome,
                super::RemovalOutcome::Missing | super::RemovalOutcome::Removed
            )
        {
            let _ = super::removal::resume_pending();
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn reserve_exclusive_mutation(id: &str) -> Result<ExclusiveComponentMutation> {
    validate_identifier(id)?;
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    if state.pending_removals.contains(id)
        || state.removals_in_progress.contains(id)
        || state.counts.get(id).copied().unwrap_or_default() != 0
    {
        anyhow::bail!("component is currently in use or pending removal");
    }
    state.removals_in_progress.insert(id.to_string());
    Ok(ExclusiveComponentMutation { id: id.to_string() })
}

#[cfg(not(feature = "recorder-worker"))]
impl Drop for ExclusiveComponentMutation {
    fn drop(&mut self) {
        LEASES
            .lock()
            .unwrap_or_else(|value| value.into_inner())
            .removals_in_progress
            .remove(&self.id);
    }
}

pub(super) fn reserve_removal(id: &str) -> bool {
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    state.pending_removals.insert(id.to_string());
    if state.counts.get(id).copied().unwrap_or_default() != 0
        || state.removals_in_progress.contains(id)
    {
        true
    } else {
        state.removals_in_progress.insert(id.to_string());
        false
    }
}

pub(super) fn finish_removal(id: &str, clear_pending: bool) {
    let mut state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    state.removals_in_progress.remove(id);
    if clear_pending {
        state.pending_removals.remove(id);
    }
}

pub(super) fn removal_pending(id: &str) -> bool {
    let state = LEASES.lock().unwrap_or_else(|value| value.into_inner());
    state.pending_removals.contains(id) || state.removals_in_progress.contains(id)
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
        assert!(!reserve_removal(id));
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
}
