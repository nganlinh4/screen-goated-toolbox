use super::*;

#[test]
fn retirement_returns_completed_snapshot_and_cancels_running_snapshot() {
    for state in ["completed", "running"] {
        let job_id = format!("test-retire-subtitle-{state}");
        let snapshot = Arc::new(Mutex::new(SubtitleJobSnapshot {
            state: state.to_string(),
            results: vec![SubtitleClipResult {
                clip_id: "clip".to_string(),
                is_partial: false,
                segments: vec![SubtitleSegmentResult {
                    start_time: 0.0,
                    end_time: 1.0,
                    text: "result".to_string(),
                }],
            }],
            ..SubtitleJobSnapshot::default()
        }));
        let cancelled = Arc::new(AtomicBool::new(false));
        subtitle_jobs().lock().unwrap().insert(
            job_id.clone(),
            JobHandle::new(snapshot.clone(), cancelled.clone()),
        );
        let response =
            handle_cancel_subtitle_generation(&serde_json::json!({ "jobId": job_id })).unwrap();
        if state == "completed" {
            assert_eq!(response["state"], "completed");
            assert_eq!(response["results"][0]["segments"][0]["text"], "result");
            assert!(!cancelled.load(Ordering::SeqCst));
            assert_eq!(snapshot.lock().unwrap().state, "completed");
        } else {
            assert!(response.is_null());
            assert!(cancelled.load(Ordering::SeqCst));
            assert_eq!(snapshot.lock().unwrap().state, "cancelled");
        }
        subtitle_jobs().lock().unwrap().remove(&job_id);
    }
}

#[test]
fn cancellation_before_start_fences_the_allocated_transcription_ticket() {
    let allocation =
        job_registry::allocate_narration_job_id(&serde_json::json!({ "kind": "transcription" }))
            .unwrap();
    let job_id = allocation["jobId"].as_str().unwrap();
    let response =
        handle_cancel_subtitle_generation(&serde_json::json!({ "jobId": job_id })).unwrap();
    assert_eq!(response["outcome"], "missing");
    let mut requests = job_registry::client_requests().unwrap();
    assert!(
        job_registry::claim_client_start(&mut requests, job_id, NarrationKind::Transcription)
            .is_err()
    );
}
