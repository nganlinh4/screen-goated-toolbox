use anyhow::Result;

use super::{RemovalOutcome, request_remove_with_policy};
use crate::component_registry::{self, lease, pending};

pub(super) fn resume_pending() -> Result<Vec<(String, RemovalOutcome)>> {
    let mut remaining = match pending::snapshot() {
        Ok((pending, unresolved)) => {
            for id in unresolved {
                crate::log_info!(
                    "[Components] pending removal {id} is not supported by this host; retaining it"
                );
            }
            pending
        }
        Err(error) => {
            lease::start_pending_scan();
            return Err(error);
        }
    };
    let mut outcomes = Vec::new();
    let mut first_error = None;
    while !remaining.is_empty() {
        let mut blocked = Vec::new();
        let mut progress = false;
        for id in remaining {
            let outcome = match retry_pending_removal(&id) {
                Ok(Some(outcome)) => outcome,
                Ok(None) => continue,
                Err(error) => {
                    crate::log_info!("[Components] could not resume removal of {id}: {error:#}");
                    lease::start_pending_watcher(&id);
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(
                            "could not resume removal of {id}: {error:#}"
                        ));
                    }
                    continue;
                }
            };
            if matches!(outcome, RemovalOutcome::RequiredBy(_)) {
                lease::start_pending_watcher(&id);
                blocked.push((id, outcome));
            } else {
                progress |= matches!(outcome, RemovalOutcome::Missing | RemovalOutcome::Removed);
                outcomes.push((id, outcome));
            }
        }
        if !progress {
            outcomes.extend(blocked);
            break;
        }
        remaining = blocked.into_iter().map(|(id, _)| id).collect();
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(outcomes),
    }
}

pub(super) fn retry_pending_removal(id: &str) -> Result<Option<RemovalOutcome>> {
    let _mutation = component_registry::acquire_mutation_guard()?;
    if !pending::list()?.iter().any(|pending| pending == id) {
        return Ok(None);
    }
    let outcome = request_remove_with_policy(id, false, true)?;
    Ok(Some(outcome))
}
