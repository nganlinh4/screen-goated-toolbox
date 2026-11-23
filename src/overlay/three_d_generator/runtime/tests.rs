use super::*;

#[test]
fn uncertain_publication_retains_3d_inputs_without_queueing() {
    let name = format!(
        "{}::uncertain_publication_retains_3d_inputs_without_queueing",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |root| {
        let source = crate::overlay::creation_submission_test::input_image(root);
        let request = StartJobRequest {
            image_path: source.to_string_lossy().to_string(),
            source_descriptors: Vec::new(),
            output_dir: Some(root.join("output").to_string_lossy().to_string()),
            final_output_dir: None,
            polycount: 5_000,
            mode: "quality".to_string(),
            generation_mode: GenerationMode::Quality,
            output_format: "glb".to_string(),
            auto_segment: false,
            segmentation_level: "balanced".to_string(),
            topology: None,
            segmentation_mode: String::new(),
            instruction: None,
            output_name: String::new(),
            dispatch_id: String::new(),
        };
        let error = super::submission::start_job(request).unwrap_err();
        assert!(error.contains("uncertain"), "{error}");
        assert!(STATE.lock().unwrap().jobs.is_empty());
        assert!(super::admission::try_acquire().is_some());
        crate::overlay::creation_submission_test::assert_retained("3d", &source);
    });
}

#[test]
fn uncertain_refinement_retains_prior_source_and_staging() {
    let name = format!(
        "{}::uncertain_refinement_retains_prior_source_and_staging",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |root| {
        let source = prepare_refinement(root, "balanced");
        let error = super::submission::start_refinement(RefineRequest {
            continuation_id: "parent-id".into(),
            kind: RefinementKind::SeparateParts,
            segmentation_level: Some("balanced".into()),
            topology: None,
            face_limit: None,
            animation: None,
        })
        .unwrap_err();
        assert!(error.contains("uncertain"), "{error}");
        let state = STATE.lock().unwrap();
        assert_eq!(state.jobs.len(), 1);
        assert!(state.continuations.contains_key("parent-id"));
        drop(state);
        crate::overlay::creation_submission_test::assert_retained("3d", &source);
    });
}

pub(super) fn prepare_refinement(root: &std::path::Path, level: &str) -> PathBuf {
    let source = crate::overlay::creation_submission_test::input_image(root);
    let inspected = crate::overlay::creation_source::inspect_image(&source).unwrap();
    let snapshot =
        crate::overlay::creation_source_snapshot::prepare("3d", "parent-dispatch", &[inspected])
            .unwrap();
    let descriptor = snapshot.descriptors()[0].clone();
    let expiry = now_ms() + 60_000;
    crate::overlay::creation_source_snapshot::retain_continuation(
        std::slice::from_ref(&descriptor),
        "parent-dispatch",
        "parent-id",
        expiry,
    )
    .unwrap();
    snapshot.persist();
    let output = root.join("output");
    std::fs::create_dir_all(&output).unwrap();
    let output = std::fs::canonicalize(output).unwrap();
    let glb = output.join("previous.glb");
    let mut json = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]}"#.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + 36;
    let mut bytes = Vec::from(&b"glTF\x02\0\0\0"[..]);
    bytes.extend_from_slice(&(total as u32).to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x4e4f_534au32.to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&36u32.to_le_bytes());
    bytes.extend_from_slice(&0x004e_4942u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 36]);
    std::fs::write(&glb, bytes).unwrap();
    let mut previous = continuation();
    previous.segmentation_level = level.to_string();
    previous.available_actions.push("add_materials".into());
    previous.image_path = descriptor.path.clone();
    previous.source_descriptor = descriptor;
    previous.output_dir = output;
    previous.previous_output_path = glb;
    previous.expires_at_ms = expiry;
    {
        let mut state = STATE.lock().unwrap();
        state
            .continuations
            .insert("parent-id".to_string(), previous);
        state.jobs.insert("parent-id".to_string(), idle_status());
    }
    source
}

const _: () = assert!(MAX_RETAINED_TERMINAL_JOBS >= MAX_QUEUED_JOBS);

#[test]
fn bounded_user_interaction_remains_an_active_job() {
    assert!(status_is_busy("waiting_for_user"));
}

#[test]
fn queued_status_with_a_live_process_is_not_dispatched_or_ignored_for_capacity() {
    let mut state = RuntimeState::default();
    let mut active = idle_status();
    active.stage = "queued".to_string();
    state.insert_job("active".to_string(), active);
    state.pids.insert("active".to_string(), 123);
    let mut waiting = idle_status();
    waiting.stage = "queued".to_string();
    state.insert_job("waiting".to_string(), waiting);
    assert_eq!(state.running_count(), 1);
    assert_eq!(state.next_queued_job().as_deref(), Some("waiting"));
}

pub(super) fn continuation() -> Continuation {
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
        segmentation_level: "detailed".to_string(),
        topology: None,
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
    let newest = format!("queued-{:02}", MAX_QUEUED_JOBS - 1);
    assert_eq!(
        state.job_order.first().map(String::as_str),
        Some("queued-00")
    );
    assert_eq!(
        state.job_order.last().map(String::as_str),
        Some(newest.as_str())
    );
}

#[test]
fn segmented_revisions_keep_their_advertised_continuation() {
    let mut state = RuntimeState::default();
    let mut current = continuation();
    current.is_segmented = true;
    current.supported_actions = vec!["add_materials".to_string()];
    current
        .available_actions
        .clone_from(&current.supported_actions);
    state.continuations.insert("current".to_string(), current);
    let retained = state.take_continuation("current", 0).unwrap();
    assert!(retained.is_segmented);
    assert_eq!(retained.available_actions, vec!["add_materials"]);
    assert!(state.take_continuation("current", 0).is_err());
}
