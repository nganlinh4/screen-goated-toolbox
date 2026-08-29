use super::*;

const _: () = assert!(MAX_RETAINED_TERMINAL_JOBS >= MAX_QUEUED_JOBS);

#[test]
fn supports_two_parallel_jobs() {
    assert_eq!(MAX_PARALLEL_JOBS, 2);
}

#[test]
fn every_submission_gets_fresh_job_and_dispatch_ids() {
    assert_ne!(next_job_id().unwrap(), next_job_id().unwrap());
    assert_ne!(next_dispatch_id().unwrap(), next_dispatch_id().unwrap());
}

#[test]
fn simultaneous_same_source_jobs_freeze_distinct_output_names() {
    let first = crate::overlay::creation_output::assigned_name(
        "same.png",
        &next_dispatch_id().unwrap(),
        None,
        "svg",
    )
    .unwrap();
    let second = crate::overlay::creation_output::assigned_name(
        "same.png",
        &next_dispatch_id().unwrap(),
        None,
        "svg",
    )
    .unwrap();
    assert_ne!(first, second);
}

#[test]
fn queued_jobs_are_never_pruned_as_terminal_history() {
    let mut state = RuntimeState::default();
    for index in 0..MAX_QUEUED_JOBS {
        let id = format!("queued-{index:02}");
        state.order.push(id.clone());
        state.jobs.insert(
            id.clone(),
            JobStatus {
                job_id: id,
                stage: "queued".to_string(),
                progress_text: "Queued".to_string(),
                progress_key: Some("svg.queued".to_string()),
                phase: Some("queued".to_string()),
                elapsed_ms: Some(0),
                estimated_total_ms: None,
                progress_ratio: Some(0.0),
                output_path: None,
                output_name: None,
                source_image_path: "source.png".to_string(),
                output_dir: "output".to_string(),
                model: "simple".to_string(),
                background_mode: "opaque".to_string(),
                error: None,
            },
        );
        state.prune_terminal_jobs();
    }
    assert_eq!(state.jobs.len(), MAX_QUEUED_JOBS);
    assert_eq!(state.order.first().map(String::as_str), Some("queued-00"));
    assert_eq!(state.order.last().map(String::as_str), Some("queued-49"));
}

#[test]
fn transparent_background_modes_are_normalized_without_changing_the_legacy_default() {
    assert_eq!(default_background_mode(), "opaque");
    assert_eq!(normalize_background_mode("auto"), "auto");
    assert_eq!(normalize_background_mode("transparent"), "transparent");
    assert_eq!(normalize_background_mode("opaque"), "opaque");
    assert_eq!(normalize_background_mode("unknown"), "opaque");
    assert!(background_is_opaque("opaque"));
    assert!(!background_is_opaque("transparent"));
}
