//! One-capture auxiliary reads with replayable raw provider output.

use super::*;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

const PROVIDER_TIMEOUT: Duration = Duration::from_secs(10);

fn accept_any_candidate(_: &str) -> bool {
    true
}

pub(super) fn read_plain(
    view: View,
    question: &str,
    ctx: &str,
    cancel: &AtomicBool,
    prefer: &[&str],
) -> Result<String> {
    use super::super::super::telemetry::{self, Privacy};

    let request_id = telemetry::next_artifact_id();
    let started = Instant::now();
    let cap = session::capture_virtual().inspect_err(|error| {
        telemetry::typed_error(
            "ERR_VISION_CAPTURE_FAILED",
            "vision",
            "failed to capture the exact input for an auxiliary vision request",
            json!({"request_id": request_id, "error": error.to_string()}),
        );
    })?;
    let (jpeg, shown) = session::encode_view(&cap, view, VISION_SHORT, None, None, None)
        .inspect_err(|error| {
            telemetry::typed_error(
                "ERR_VISION_ENCODE_FAILED",
                "vision",
                "failed to encode the auxiliary vision input",
                json!({"request_id": request_id, "error": error.to_string()}),
            );
        })?;
    let shown_array = [shown.x, shown.y, shown.w, shown.h];
    let input_sha256 = format!("{:x}", Sha256::digest(&jpeg));
    let resolved_candidates = super::super::super::vision_reader::configured_general_chain(prefer);
    let input_artifact = format!("vision-input-{request_id:06}.jpg");
    let input_write_ok = write_artifact("vision_input", &input_artifact, &jpeg, None);
    let vision_bundle_artifact = format!("vision-bundle-{request_id:06}.json");
    let vision_bundle = serde_json::to_vec_pretty(&json!({
        "schema_version": 3,
        "session_id": telemetry::session_id(),
        "turn_id": telemetry::current_turn(),
        "request_id": request_id,
        "question": question,
        "context": ctx,
        "preferred_models": prefer,
        "resolved_candidate_models": &resolved_candidates,
        "input_sha256": &input_sha256,
        "image_artifact": &input_artifact,
        "view": shown_array,
    }))?;
    let vision_bundle_sha256 = format!("{:x}", Sha256::digest(&vision_bundle));
    let bundle_write_ok = write_artifact(
        "vision_bundle",
        &vision_bundle_artifact,
        &vision_bundle,
        None,
    );
    emit_event(
        None,
        "vision_request",
        Privacy::UserText,
        json!({
            "request_id": request_id,
            "question_preview": question.chars().take(200).collect::<String>(),
            "preferred_models": prefer,
            "resolved_candidate_models": resolved_candidates,
            "byte_count": jpeg.len(),
            "input_sha256": input_sha256,
            "view": shown_array,
            "artifact_path": input_artifact,
            "artifact_write_ok": input_write_ok,
            "bundle_artifact_path": vision_bundle_artifact,
            "bundle_sha256": vision_bundle_sha256,
            "bundle_write_ok": bundle_write_ok,
        }),
    );

    let question = question.to_string();
    let context = ctx.to_string();
    let prefer = prefer
        .iter()
        .map(|model| model.to_string())
        .collect::<Vec<_>>();
    let completed_attempts: Arc<Mutex<Vec<super::super::super::vision_reader::CandidateAttempt>>> =
        Arc::new(Mutex::new(Vec::new()));
    let worker_attempts = Arc::clone(&completed_attempts);
    let report = run_cancellable(cancel, move || {
        let prefer = prefer.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(super::super::super::vision_reader::read_image_pref_where(
            &jpeg,
            &question,
            &context,
            &prefer,
            Some(Arc::new(AtomicBool::new(false))),
            PROVIDER_TIMEOUT,
            super::super::super::vision_reader::CandidateCallbacks::new(
                move |attempt: &super::super::super::vision_reader::CandidateAttempt| {
                    if let Ok(mut completed) = worker_attempts.lock() {
                        completed.push(attempt.clone());
                    }
                },
                accept_any_candidate,
            ),
        ))
    });
    let (answer, attempts): (
        Result<String>,
        Vec<super::super::super::vision_reader::CandidateAttempt>,
    ) = match report {
        Ok(report) => (report.answer.map_err(anyhow::Error::msg), report.attempts),
        Err(error) => (
            Err(error),
            completed_attempts
                .lock()
                .map(|attempts| attempts.clone())
                .unwrap_or_default(),
        ),
    };
    let (candidate_manifest_artifact, candidate_artifacts_write_ok) = persist_candidates(
        request_id,
        &input_sha256,
        &vision_bundle_sha256,
        &attempts,
        None,
    );
    emit_event(
        None,
        "vision_result",
        Privacy::UserText,
        json!({
            "request_id": request_id,
            "ok": answer.is_ok(),
            "duration_ms": started.elapsed().as_millis(),
            "candidate_attempt_count": attempts.len(),
            "candidate_response_count": attempts.iter().filter(|attempt| attempt.response.is_some()).count(),
            "candidate_manifest_artifact_path": candidate_manifest_artifact,
            "artifacts_persisted": input_write_ok && bundle_write_ok && candidate_artifacts_write_ok,
            "input_sha256": input_sha256,
            "bundle_sha256": vision_bundle_sha256,
            "error": answer.as_ref().err().map(ToString::to_string),
        }),
    );
    answer
}

pub(super) fn persist_candidates(
    request_id: u64,
    input_sha256: &str,
    bundle_sha256: &str,
    attempts: &[super::super::super::vision_reader::CandidateAttempt],
    action: Option<super::super::super::telemetry::ActionTrace>,
) -> (String, bool) {
    let directory = super::super::super::telemetry::trace_dir();
    let (records, responses_write_ok) = match super::candidate_artifacts::write_candidate_responses(
        &directory, request_id, attempts,
    ) {
        Ok(records) => (records, true),
        Err(error) => {
            super::super::super::telemetry::artifact_write_failed(
                "vision_candidate_response",
                &directory,
                action,
                &error,
            );
            (Vec::new(), false)
        }
    };
    let name = format!("vision-candidates-{request_id:06}.json");
    let manifest = super::super::super::telemetry::artifact_record(
        "vision_candidates",
        request_id,
        action,
        json!({
            "input_sha256": input_sha256,
            "bundle_sha256": bundle_sha256,
            "attempts": records,
        }),
    );
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
    let manifest_write_ok = write_artifact("vision_candidates", &name, &bytes, action);
    (name, responses_write_ok && manifest_write_ok)
}

pub(super) fn emit_event(
    action: Option<super::super::super::telemetry::ActionTrace>,
    event: &str,
    privacy: super::super::super::telemetry::Privacy,
    fields: Value,
) {
    if let Some(action) = action {
        super::super::super::telemetry::event_for_action(event, "vision", privacy, action, fields);
    } else {
        super::super::super::telemetry::event(event, "vision", privacy, fields);
    }
}

pub(super) fn write_artifact(
    kind: &str,
    name: &str,
    bytes: &[u8],
    action: Option<super::super::super::telemetry::ActionTrace>,
) -> bool {
    let path = super::super::super::telemetry::trace_dir().join(name);
    match write_new(&path, bytes) {
        Ok(()) => true,
        Err(error) => {
            super::super::super::telemetry::artifact_write_failed(kind, &path, action, &error);
            false
        }
    }
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::super::super::vision_reader::CandidateAttempt;

    #[test]
    fn raw_candidates_are_replayable() {
        let directory = std::env::temp_dir().join(format!(
            "sgt-vision-candidates-{}-{}",
            std::process::id(),
            super::super::super::super::telemetry::next_artifact_id()
        ));
        let attempts = vec![
            CandidateAttempt {
                model_id: "candidate-a".into(),
                provider: "provider-a".into(),
                response: Some("first raw response".into()),
                error: None,
                accepted: false,
            },
            CandidateAttempt {
                model_id: "candidate-b".into(),
                provider: "provider-b".into(),
                response: Some("selected raw response".into()),
                error: None,
                accepted: true,
            },
        ];
        let records =
            super::super::candidate_artifacts::write_candidate_responses(&directory, 7, &attempts)
                .unwrap();
        assert_eq!(records.len(), 2);
        for (index, expected) in ["first raw response", "selected raw response"]
            .iter()
            .enumerate()
        {
            let path = records[index]["response_artifact"]["path"]
                .as_str()
                .unwrap();
            assert_eq!(
                std::fs::read_to_string(directory.join(path)).unwrap(),
                *expected
            );
        }
        assert_eq!(records[0]["accepted"], false);
        assert_eq!(records[1]["accepted"], true);
        let _ = std::fs::remove_dir_all(directory);
    }
}
