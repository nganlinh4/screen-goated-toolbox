//! Conservative ownership of interrupted effects until a trusted receipt settles them.
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::reader::{Pending, Reader};

#[derive(Clone, Debug)]
pub(super) struct JobIdentity {
    id: String,
    cancel: Arc<AtomicBool>,
}

impl JobIdentity {
    pub(super) fn from_pending(pending: &Pending) -> Option<Self> {
        Some(Self {
            id: pending.id.clone()?,
            cancel: pending.cancel.clone()?,
        })
    }
    pub(super) fn matches(&self, id: &str, cancel: &Arc<AtomicBool>) -> bool {
        self.id == id && Arc::ptr_eq(&self.cancel, cancel)
    }
    pub(super) fn same(&self, other: &Self) -> bool {
        self.matches(&other.id, &other.cancel)
    }
}

#[derive(Default)]
pub(super) enum Reconciliation {
    #[default]
    Clear,
    Pending {
        owner: JobIdentity,
        proven_not_dispatched: bool,
    },
    Unknown,
}

impl Reconciliation {
    pub(super) fn required(&self) -> bool {
        !matches!(self, Self::Clear)
    }
    pub(super) fn unresolved(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// Returns true if this introduces an unresolvable obligation.
    pub(super) fn require_pending(&mut self, pending: &Pending) -> bool {
        let Some(identity) = JobIdentity::from_pending(pending) else {
            return self.require_unknown();
        };
        match self {
            Self::Clear => {
                *self = Self::Pending {
                    owner: identity,
                    proven_not_dispatched: false,
                };
                false
            }
            Self::Pending { owner: current, .. } if current.same(&identity) => false,
            _ => self.require_unknown(),
        }
    }

    pub(super) fn require_unknown(&mut self) -> bool {
        let changed = !matches!(self, Self::Unknown);
        *self = Self::Unknown;
        changed
    }

    /// The marker covers the requested dispatch, not unrelated turn cleanup.
    pub(super) fn settle(
        &mut self,
        id: &str,
        cancel: &Arc<AtomicBool>,
        not_dispatched: bool,
        browser_unsettled: bool,
    ) -> Option<JobIdentity> {
        let Self::Pending {
            owner,
            proven_not_dispatched,
        } = self
        else {
            return None;
        };
        if !owner.matches(id, cancel) {
            return None;
        }
        if !not_dispatched {
            self.require_unknown();
            return None;
        }
        *proven_not_dispatched = true;
        self.discharge_if_quiet(browser_unsettled)
    }

    pub(super) fn discharge_if_quiet(&mut self, browser_unsettled: bool) -> Option<JobIdentity> {
        if browser_unsettled {
            return None;
        }
        let Self::Pending {
            owner,
            proven_not_dispatched: true,
        } = self
        else {
            return None;
        };
        let owner = owner.clone();
        *self = Self::Clear;
        Some(owner)
    }
}

pub(super) fn warn_unresolved() {
    super::overlay::push_log("[warn] The earlier action may have taken effect. Automatic reconciliation is unavailable; inspect the affected state before starting another session.".to_string());
}

pub(super) fn discharge_if_quiet(state: &mut Reader, browser_unsettled: bool) {
    if let Some(owner) = state.reconciliation.discharge_if_quiet(browser_unsettled) {
        state.turn_outcomes.clear_interruption_for(&owner);
    }
}

#[cfg(test)]
#[path = "reconciliation_tests.rs"]
mod tests;
