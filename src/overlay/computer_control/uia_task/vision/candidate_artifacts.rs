//! Persist raw provider responses for replay and diagnosis.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn write_candidate_responses(
    directory: &Path,
    request_id: u64,
    attempts: &[super::super::super::vision_reader::CandidateAttempt],
) -> std::io::Result<Vec<Value>> {
    std::fs::create_dir_all(directory)?;
    attempts
        .iter()
        .enumerate()
        .map(|(index, attempt)| {
            let artifact = if let Some(response) = &attempt.response {
                let name = format!(
                    "vision-response-{request_id:06}-candidate-{:02}.txt",
                    index + 1
                );
                super::frozen_read::write_new(&directory.join(&name), response.as_bytes())?;
                Some(json!({
                    "path": name,
                    "sha256": format!("{:x}", Sha256::digest(response.as_bytes())),
                    "byte_count": response.len(),
                    "char_count": response.chars().count(),
                }))
            } else {
                None
            };
            Ok(json!({
                "ordinal": index + 1,
                "model_id": attempt.model_id,
                "provider": attempt.provider,
                "accepted": attempt.accepted,
                "selected_for_answer": attempt.accepted,
                "response_artifact": artifact,
                "error": attempt.error,
            }))
        })
        .collect()
}
