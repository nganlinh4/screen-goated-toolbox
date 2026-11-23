use super::{StagedUpdate, UpdateCandidate, UpdateStatus, bump_is_greater};
use anyhow::Result;
use std::sync::{Arc, Mutex, mpsc::Sender};

#[derive(Clone)]
pub(super) struct Operation(Arc<()>);

#[derive(Debug, Clone)]
pub struct UpdateEvent {
    identity: Arc<()>,
    status: UpdateStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Checking,
    Available,
    UpToDate,
    Downloading,
    Ready,
    Error,
    Closed,
}

struct Selection {
    phase: Phase,
    operation: Option<Operation>,
    candidate: Option<UpdateCandidate>,
    event: Arc<()>,
    delivered: bool,
    auto_started: bool,
}

pub(super) struct UpdateOwner {
    state: Mutex<Selection>,
    tx: Sender<UpdateEvent>,
}

impl UpdateOwner {
    pub(super) fn new(tx: Sender<UpdateEvent>) -> Self {
        Self {
            tx,
            state: Mutex::new(Selection {
                phase: Phase::Idle,
                operation: None,
                candidate: None,
                event: Arc::new(()),
                delivered: true,
                auto_started: false,
            }),
        }
    }

    fn publish(&self, state: &mut Selection, status: UpdateStatus) {
        state.event = Arc::new(());
        state.delivered = false;
        let _ = self.tx.send(UpdateEvent {
            identity: Arc::clone(&state.event),
            status,
        });
    }

    pub(super) fn begin_check(&self, automatic: bool) -> Option<Operation> {
        let mut state = self.state.lock().ok()?;
        if automatic {
            if state.auto_started {
                return None;
            }
            state.auto_started = true;
        }
        if matches!(
            state.phase,
            Phase::Checking | Phase::Downloading | Phase::Ready | Phase::Closed
        ) {
            return None;
        }
        let operation = Operation(Arc::new(()));
        state.operation = Some(operation.clone());
        state.candidate = None;
        state.phase = Phase::Checking;
        self.publish(&mut state, UpdateStatus::Checking);
        Some(operation)
    }

    fn owns(state: &Selection, operation: &Operation, phase: Phase) -> bool {
        state.phase == phase
            && state
                .operation
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(&active.0, &operation.0))
    }

    pub(super) fn finish_check(&self, operation: &Operation, result: Result<UpdateCandidate>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !Self::owns(&state, operation, Phase::Checking) {
            return;
        }
        state.operation = None;
        let result = result.and_then(|candidate| {
            Ok((
                bump_is_greater(env!("CARGO_PKG_VERSION"), &candidate.version)?,
                candidate,
            ))
        });
        let status = match result {
            Ok((true, candidate)) => {
                state.phase = Phase::Available;
                let status = UpdateStatus::UpdateAvailable {
                    version: candidate.version.to_string(),
                    body: candidate.body.clone(),
                };
                state.candidate = Some(candidate);
                status
            }
            Ok((false, _)) => {
                state.phase = Phase::UpToDate;
                UpdateStatus::UpToDate(env!("CARGO_PKG_VERSION").into())
            }
            Err(error) => {
                state.phase = Phase::Error;
                UpdateStatus::Error(error.to_string())
            }
        };
        self.publish(&mut state, status);
    }

    pub(super) fn begin_download(&self) -> Option<(Operation, UpdateCandidate)> {
        let mut state = self.state.lock().ok()?;
        if state.phase != Phase::Available {
            return None;
        }
        let candidate = state.candidate.take()?;
        let operation = Operation(Arc::new(()));
        state.operation = Some(operation.clone());
        state.phase = Phase::Downloading;
        self.publish(&mut state, UpdateStatus::Downloading);
        Some((operation, candidate))
    }

    pub(super) fn finish_download(&self, operation: &Operation, result: Result<StagedUpdate>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !Self::owns(&state, operation, Phase::Downloading) {
            return;
        }
        state.operation = None;
        let status = match result {
            Ok(staged) => {
                state.phase = Phase::Ready;
                UpdateStatus::UpdatedAndRestartRequired(staged)
            }
            Err(error) => {
                state.phase = Phase::Error;
                UpdateStatus::Error(error.to_string())
            }
        };
        self.publish(&mut state, status);
    }

    pub(super) fn accept(&self, event: UpdateEvent) -> Option<UpdateStatus> {
        let mut state = self.state.lock().ok()?;
        if state.phase == Phase::Closed
            || state.delivered
            || !Arc::ptr_eq(&state.event, &event.identity)
        {
            return None;
        }
        state.delivered = true;
        Some(event.status)
    }

    pub(super) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.phase = Phase::Closed;
            state.operation = None;
            state.candidate = None;
            state.event = Arc::new(());
        }
    }
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
