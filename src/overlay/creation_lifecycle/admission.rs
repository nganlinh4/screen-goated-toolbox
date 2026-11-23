use std::sync::{Mutex, MutexGuard, TryLockError};

pub(crate) fn count_pending(
    product: &str,
    in_memory: impl IntoIterator<Item = String>,
) -> Result<usize, String> {
    let mut jobs = in_memory
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    jobs.extend(
        crate::overlay::creation_intent_journal::load(product)?
            .into_iter()
            .map(|intent| intent.job_id),
    );
    Ok(jobs.len())
}

pub(crate) fn register_admission<S, U, T, R>(
    state: &Mutex<S>,
    prepare: impl FnOnce(&S) -> Result<U, String>,
    persist: impl FnOnce(U) -> Result<T, String>,
    attach: impl FnOnce(&mut S, T) -> R,
) -> Result<R, String> {
    let prepared = {
        let state = state
            .lock()
            .map_err(|_| "Creation job state is unavailable.".to_string())?;
        prepare(&state)?
    };
    let accepted = persist(prepared)?;
    let mut state = state
        .lock()
        .map_err(|_| "Creation job state is unavailable.".to_string())?;
    Ok(attach(&mut state, accepted))
}

#[derive(Default)]
pub(crate) struct AdmissionGate(Mutex<()>);

impl AdmissionGate {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(()))
    }

    pub(crate) fn acquire(&self) -> MutexGuard<'_, ()> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub(crate) fn try_acquire(&self) -> Option<MutexGuard<'_, ()>> {
        match self.0.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc::channel};
    use std::time::Duration;

    #[test]
    fn queued_recovery_registration_releases_state_during_persistence() {
        let state = Arc::new(Mutex::new(vec!["existing"]));
        let background = state.clone();
        let (started, wait_started) = channel();
        let (release, wait_release) = channel();
        let worker = std::thread::spawn(move || {
            register_admission(
                &background,
                |state| Ok(state.len()),
                |count| {
                    assert_eq!(count, 1);
                    started.send(()).unwrap();
                    wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok("accepted")
                },
                |state, accepted| state.push(accepted),
            )
            .unwrap();
        });
        wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
        state
            .try_lock()
            .expect("Persistence held the worker state")
            .push("progress");
        release.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(*state.lock().unwrap(), ["existing", "progress", "accepted"]);
    }

    #[test]
    fn queued_recovery_capacity_counts_registered_and_retained_jobs_once() {
        use crate::overlay::creation_submission_test::queued::*;
        let name = format!(
            "{}::queued_recovery_capacity_counts_registered_and_retained_jobs_once",
            module_path!().split_once("::").unwrap().1
        );
        crate::overlay::creation_submission_test::run_isolated_without_fault(&name, |root| {
            for case in fixture()["capacityCases"].as_array().unwrap() {
                let saved = (0..2)
                    .map(|_| {
                        seed(
                            root,
                            "image",
                            "png",
                            false,
                            serde_json::json!({"prompt":"sample"}),
                            |value| value,
                        )
                    })
                    .collect::<Vec<_>>();
                let accepted = case["accepted"].as_array().unwrap();
                for (index, record) in saved.iter().enumerate() {
                    if !accepted.contains(&serde_json::json!(index)) {
                        crate::overlay::creation_intent_journal::clear_required(
                            "image",
                            &record.job_id,
                        )
                        .unwrap();
                    }
                }
                let registered = case["registered"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| saved[value.as_u64().unwrap() as usize].job_id.clone())
                    .collect::<Vec<_>>();
                assert_eq!(
                    count_pending("image", registered).unwrap(),
                    case["expectedCount"].as_u64().unwrap() as usize
                );
                for intent in crate::overlay::creation_intent_journal::load("image").unwrap() {
                    crate::overlay::creation_intent_journal::clear_required(
                        "image",
                        &intent.job_id,
                    )
                    .unwrap();
                }
            }
        });
    }
}
