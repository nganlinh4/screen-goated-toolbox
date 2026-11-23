use std::path::PathBuf;

use super::STATE;

pub(in crate::overlay::three_d_generator) fn remap_result_path(previous: &str, current: &str) {
    let current_name = PathBuf::from(current)
        .file_name()
        .map(|name| name.to_string_lossy().to_string());
    if let Ok(mut state) = STATE.lock() {
        for status in state.jobs.values_mut() {
            if status
                .output_path
                .as_deref()
                .is_some_and(|path| path.eq_ignore_ascii_case(previous))
            {
                status.output_path = Some(current.to_string());
                status.output_name = current_name.clone();
            }
        }
        for continuation in state.continuations.values_mut() {
            if continuation
                .previous_output_path
                .to_string_lossy()
                .eq_ignore_ascii_case(previous)
            {
                continuation.previous_output_path = PathBuf::from(current);
            }
        }
    }
}

pub(in crate::overlay::three_d_generator) fn forget_result_path(path: &str) {
    if let Ok(mut state) = STATE.lock() {
        for status in state.jobs.values_mut() {
            if status
                .output_path
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(path))
            {
                status.output_path = None;
                status.output_name = None;
                status.can_segment = false;
            }
        }
        let removed = state
            .continuations
            .iter()
            .filter(|(_, continuation)| {
                continuation
                    .previous_output_path
                    .to_string_lossy()
                    .eq_ignore_ascii_case(path)
            })
            .map(|(job_id, _)| job_id.clone())
            .collect::<Vec<_>>();
        for job_id in removed {
            if let Some(continuation) = state.continuations.remove(&job_id) {
                let _ = crate::overlay::creation_source_snapshot::release_continuation(
                    std::slice::from_ref(&continuation.source_descriptor),
                    &job_id,
                );
            }
        }
    }
}

pub(in crate::overlay::three_d_generator) fn is_known_result_path(path: &std::path::Path) -> bool {
    let session_paths = STATE
        .lock()
        .map(|state| {
            state
                .jobs
                .values()
                .filter_map(|status| status.output_path.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let known_in_session = session_paths
        .iter()
        .any(|value| std::fs::canonicalize(value).ok().as_deref() == Some(path));
    known_in_session
        || crate::overlay::generation_history::list("3d").is_ok_and(|entries| {
            entries.iter().any(|entry| {
                std::fs::canonicalize(&entry.output_path).ok().as_deref() == Some(path)
            })
        })
}

pub(in crate::overlay::three_d_generator) use super::cancellation::{
    cancel_for_shutdown, cancel_job,
};
