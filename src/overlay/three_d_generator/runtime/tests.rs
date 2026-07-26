use super::*;

fn continuation(group: &str) -> Continuation {
    Continuation {
        token: "opaque-token".to_string(),
        group: group.to_string(),
        image_path: "image.png".to_string(),
        output_dir: PathBuf::from("output"),
        previous_output_path: PathBuf::from("model.glb"),
        preview_path: None,
        provider: provider::ModelProvider::Tripo,
    }
}

#[test]
fn consuming_separation_invalidates_every_model_in_the_group() {
    let mut state = RuntimeState::default();
    for (job_id, group) in [
        ("first", "shared"),
        ("second", "shared"),
        ("other", "other"),
    ] {
        let mut status = idle_status();
        status.can_segment = true;
        state.jobs.insert(job_id.to_string(), status);
        state
            .continuations
            .insert(job_id.to_string(), continuation(group));
    }

    state.invalidate_continuation_group("shared");

    assert!(!state.continuations.contains_key("first"));
    assert!(!state.continuations.contains_key("second"));
    assert!(state.continuations.contains_key("other"));
    assert!(!state.jobs["first"].can_segment);
    assert!(!state.jobs["second"].can_segment);
    assert!(state.jobs["other"].can_segment);
}
