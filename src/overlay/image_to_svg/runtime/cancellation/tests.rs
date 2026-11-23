use super::*;

fn isolated(name: &str, run: impl FnOnce()) {
    let name = format!("{}::{name}", module_path!().split_once("::").unwrap().1);
    crate::overlay::creation_submission_test::run_isolated(&name, |_| run());
}

fn setup() {
    let mut request: StartJobRequest = serde_json::from_value(serde_json::json!({
        "imagePath":"source.png", "imagePaths":[], "outputDir":null,
        "model":"simple", "backgroundMode":"opaque", "prompt":"sample"
    }))
    .unwrap();
    request.dispatch_id = "dispatch".into();
    request.output_name = "result.svg".into();
    let mut state = STATE.lock().unwrap();
    *state = RuntimeState::default();
    state.jobs.insert(
        "job".into(),
        JobStatus {
            job_id: "job".into(),
            stage: "generating".into(),
            progress_text: String::new(),
            progress_key: None,
            phase: None,
            elapsed_ms: None,
            estimated_total_ms: None,
            progress_ratio: None,
            output_path: None,
            output_name: None,
            source_image_path: String::new(),
            output_dir: String::new(),
            model: String::new(),
            background_mode: String::new(),
            error: None,
        },
    );
    state.order.push("job".into());
    state
        .requests
        .insert("job".into(), (request, PathBuf::from("output")));
    state
        .request_fingerprints
        .insert("job".into(), "fingerprint".into());
    state.pids.insert("job".into(), 42);
}

#[test]
fn failed_cancel_keeps_latest_progress_and_worker_ownership() {
    isolated(
        "failed_cancel_keeps_latest_progress_and_worker_ownership",
        || {
            setup();
            let (durable, statuses) = cancel_jobs_with(
                Some("job"),
                false,
                |_| {
                    assert!(!job_cancelled("job"));
                    update_progress(
                        "job",
                        &serde_json::json!({"stage":"generating", "progressRatio":0.5}),
                    );
                    Err("storage unavailable".into())
                },
                |_| panic!("non-durable cancellation killed a worker"),
            );
            assert!(!durable);
            assert_eq!(statuses[0].stage, "generating");
            assert_eq!(statuses[0].progress_ratio, Some(0.5));
            assert_eq!(STATE.lock().unwrap().pids["job"], 42);
            assert!(STATE.lock().unwrap().requests.contains_key("job"));
        },
    );
}

#[test]
fn durable_cancel_is_terminal_and_published_outcome_preserves_the_handoff() {
    isolated(
        "durable_cancel_is_terminal_and_published_outcome_preserves_the_handoff",
        || {
            setup();
            let (durable, statuses) = cancel_jobs_with(
                Some("job"),
                false,
                |_| Ok(CancellationOutcome::Published),
                |_| panic!("publication winner killed"),
            );
            assert!(durable);
            assert_eq!(statuses[0].stage, "generating");
            assert!(!job_cancelled("job"));
            let mut killed = vec![];
            let (durable, statuses) = cancel_jobs_with(
                Some("job"),
                false,
                |_| Ok(CancellationOutcome::Cancelled),
                |pid| killed.push(pid.pid()),
            );
            assert!(durable);
            assert_eq!(statuses[0].stage, "cancelled");
            assert!(job_cancelled("job"));
            assert_eq!(killed, [42]);
            assert!(STATE.lock().unwrap().requests.is_empty());
        },
    );
}
