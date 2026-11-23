use anyhow::{Result, bail};

use super::{RemovalOutcome, resume_pending, run_reserved_removal};
use crate::component_registry::{self, catalog::validate_identifier, lease, pending};

pub(super) fn request_remove_with_policy(
    id: &str,
    resume_after_removal: bool,
    replay: bool,
) -> Result<RemovalOutcome> {
    let mutation = component_registry::acquire_mutation_guard()?;
    validate_identifier(id)?;
    if !component_registry::embedded_catalog().is_removable(id) {
        bail!("component is not removable by this host");
    }
    if !replay && let Err(error) = pending::record(id) {
        lease::start_pending_watcher(id);
        return Err(error);
    }
    let reserved = match lease::reserve_removal(id) {
        Ok(reserved) => reserved,
        Err(error) => {
            lease::start_pending_watcher(id);
            return Err(error);
        }
    };
    if reserved {
        return Ok(RemovalOutcome::Pending);
    }
    let outcome = match run_reserved_removal(id, replay) {
        Ok(outcome) => outcome,
        Err(error) => {
            lease::start_pending_watcher(id);
            return Err(error);
        }
    };
    drop(mutation);
    if resume_after_removal && matches!(outcome, RemovalOutcome::Missing | RemovalOutcome::Removed)
    {
        let _ = resume_pending();
    }
    Ok(outcome)
}
