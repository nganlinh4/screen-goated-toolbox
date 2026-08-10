use std::sync::atomic::Ordering;

use super::{READINESS_IN_FLIGHT, RUNTIME_PROCESSES, RUNTIME_SHUTTING_DOWN};

pub(crate) fn shutdown() {
    RUNTIME_SHUTTING_DOWN.store(true, Ordering::Release);
    stop_all();
}

pub(crate) fn stop_for_component_removal() {
    stop_all();
}

fn stop_all() {
    for task in READINESS_IN_FLIGHT
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .drain()
        .map(|(_, task)| task)
    {
        task.stop.store(true, Ordering::Release);
    }
    let processes = RUNTIME_PROCESSES
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .drain()
        .collect::<Vec<_>>();
    for pid in processes {
        crate::overlay::creation_recovery::terminate_process_tree(pid);
    }
}
