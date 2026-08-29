use super::*;

const _: () = assert!(MAX_RETAINED_TERMINAL_JOBS >= MAX_QUEUED_JOBS);

fn continuation() -> Continuation {
    Continuation {
        parent_dispatch_id: "parent-dispatch".to_string(),
        dispatch_id: String::new(),
        image_path: "image.png".to_string(),
        source_descriptor: crate::overlay::creation_source::SourceDescriptor {
            path: "image.png".to_string(),
            size_bytes: 1,
            sha256: "0".repeat(64),
        },
        output_dir: PathBuf::from("output"),
        staging_dir: PathBuf::from("staging"),
        output_name: "model-parts-dispatch.glb".to_string(),
        previous_output_path: PathBuf::from("model.glb"),
        generation_mode: GenerationMode::Quality,
        polycount: 5_000,
        auto_segment: false,
        instruction: Some("Keep the silhouette".to_string()),
        project_id: "project".to_string(),
        supported_actions: vec!["separate_parts".to_string()],
        available_actions: vec!["separate_parts".to_string()],
        is_segmented: false,
        is_textured: false,
        is_pbr: false,
        is_rigged: false,
        rig_type: None,
        refinement: None,
        expires_at_ms: u64::MAX,
    }
}

#[test]
fn refinement_operation_keeps_the_selected_settings_in_its_runtime_message() {
    let mut continuation = continuation();
    continuation.refinement = Some(RefineRequest {
        continuation_id: "parent".to_string(),
        kind: RefinementKind::AddMaterials,
        segmentation_level: None,
        topology: None,
        face_limit: None,
        animation: None,
    });
    let value = super::message::request_value(&RuntimeOperation::Refine { continuation });
    assert_eq!(value["kind"], "add_materials");
}

#[test]
fn expired_continuation_is_rejected_before_an_operation_can_start() {
    let mut state = RuntimeState::default();
    let mut expired = continuation();
    expired.expires_at_ms = 10;
    let mut status = idle_status();
    status.can_segment = true;
    state.jobs.insert("expired".to_string(), status);
    state.continuations.insert("expired".to_string(), expired);

    assert!(state.take_continuation("expired", 11).is_err());
    assert!(!state.jobs["expired"].can_segment);
    assert!(state.continuations.is_empty());
}

#[test]
fn runtime_continuation_expiry_is_bounded_to_twenty_four_hours() {
    let now = 1_000;
    assert_eq!(
        continuation_expiry(None, now),
        Some(now + CONTINUATION_WINDOW_MS)
    );
    assert_eq!(continuation_expiry(Some(now), now), None);
    assert_eq!(
        continuation_expiry(Some(u64::MAX), now),
        Some(now + CONTINUATION_WINDOW_MS)
    );
}

#[test]
fn every_explicit_submission_gets_a_fresh_job_id() {
    assert_ne!(next_job_id().unwrap(), next_job_id().unwrap());
    assert_ne!(next_dispatch_id().unwrap(), next_dispatch_id().unwrap());
}

#[test]
fn simultaneous_same_source_jobs_freeze_distinct_output_names() {
    let first = crate::overlay::creation_output::assigned_name(
        "same.png",
        &next_dispatch_id().unwrap(),
        None,
        "glb",
    )
    .unwrap();
    let second = crate::overlay::creation_output::assigned_name(
        "same.png",
        &next_dispatch_id().unwrap(),
        None,
        "glb",
    )
    .unwrap();
    assert_ne!(first, second);
}

#[test]
fn consuming_separation_invalidates_only_the_selected_model() {
    let mut state = RuntimeState::default();
    for job_id in ["first", "second"] {
        let mut status = idle_status();
        status.can_segment = true;
        state.jobs.insert(job_id.to_string(), status);
        state
            .continuations
            .insert(job_id.to_string(), continuation());
    }

    assert!(state.take_continuation("first", 0).is_ok());
    assert!(!state.continuations.contains_key("first"));
    assert!(state.continuations.contains_key("second"));
    assert!(!state.jobs["first"].can_segment);
    assert!(state.jobs["second"].can_segment);
}

#[test]
fn validating_a_separation_does_not_consume_its_continuation() {
    let mut state = RuntimeState::default();
    let mut status = idle_status();
    status.can_segment = true;
    state.jobs.insert("model".to_string(), status);
    state
        .continuations
        .insert("model".to_string(), continuation());

    assert!(state.peek_continuation("model", 0).is_ok());
    assert!(state.continuations.contains_key("model"));
    assert!(state.jobs["model"].can_segment);
}

#[test]
fn separation_reserves_the_retained_source_during_child_dispatch() {
    let mut continuation = continuation();
    continuation.source_descriptor.size_bytes = 8_326;
    assert_eq!(
        super::submission::retained_source_reservation(&continuation),
        (8_326, 1)
    );
}

#[test]
fn queued_jobs_are_never_pruned_as_terminal_history() {
    let mut state = RuntimeState::default();
    for index in 0..MAX_QUEUED_JOBS {
        let id = format!("queued-{index:02}");
        let mut status = idle_status();
        status.job_id = Some(id.clone());
        status.stage = "queued".to_string();
        state.insert_job(id, status);
    }
    assert_eq!(state.jobs.len(), MAX_QUEUED_JOBS);
    assert_eq!(
        state.job_order.first().map(String::as_str),
        Some("queued-00")
    );
    assert_eq!(
        state.job_order.last().map(String::as_str),
        Some("queued-49")
    );
}
