use super::*;

fn continuation(profile_dir: &str) -> Continuation {
    Continuation {
        task_id: "task".to_string(),
        profile_dir: profile_dir.to_string(),
        image_path: "image.png".to_string(),
        output_dir: PathBuf::from("output"),
        previous_output_path: PathBuf::from("model.glb"),
        preview_path: None,
        provider: provider::ModelProvider::Tripo,
    }
}

#[cfg(debug_assertions)]
#[test]
fn development_runtime_uses_the_newest_binary() {
    let older = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
    let newer = std::time::UNIX_EPOCH + std::time::Duration::from_secs(2);

    let selected = newest_dev_runtime_candidate([
        (PathBuf::from("debug.exe"), older),
        (PathBuf::from("release.exe"), newer),
    ]);

    assert_eq!(selected, Some(PathBuf::from("release.exe")));
}

#[test]
fn consuming_separation_invalidates_every_model_on_the_profile() {
    let mut state = RuntimeState::default();
    for (job_id, profile_dir) in [
        ("first", "shared"),
        ("second", "shared"),
        ("other", "other"),
    ] {
        let mut status = idle_status();
        status.can_segment = true;
        state.jobs.insert(job_id.to_string(), status);
        state
            .continuations
            .insert(job_id.to_string(), continuation(profile_dir));
    }

    state.invalidate_profile_continuations("shared");

    assert!(!state.continuations.contains_key("first"));
    assert!(!state.continuations.contains_key("second"));
    assert!(state.continuations.contains_key("other"));
    assert!(!state.jobs["first"].can_segment);
    assert!(!state.jobs["second"].can_segment);
    assert!(state.jobs["other"].can_segment);
}
