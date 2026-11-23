//! Prepared, independently assessable artifact publication transactions.

use super::{TextArtifact, json, load_text, session_storage, sha256_hex};
use crate::overlay::computer_control::executor::file_transaction::{
    CommitOptions, CommitOutcome, EditGuard, StagedFile, write_synced_sibling,
};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "export_target.rs"]
mod target;

struct Baseline {
    guard: EditGuard,
    bytes: Vec<u8>,
}

pub(in crate::overlay::computer_control) struct PreparedSave {
    artifact: TextArtifact,
    text: String,
    target: target::Target,
    baseline: Option<Baseline>,
    overwrite: bool,
}

impl PreparedSave {
    pub(in crate::overlay::computer_control) fn prepare(
        id: &str,
        path: &str,
        overwrite: bool,
    ) -> Result<Self, Value> {
        let (artifact, text) = load_text(id).map_err(|error| {
            failure("ERR_ARTIFACT_NOT_FOUND", error.to_string(), path, false, 0)
        })?;
        let requested = Path::new(path);
        if !requested.is_absolute() {
            return Err(failure(
                "ERR_ARTIFACT_PATH_NOT_ABSOLUTE",
                "path must be an absolute output path",
                path,
                false,
                0,
            ));
        }
        if requested.file_name().is_none() || requested.is_dir() {
            return Err(failure(
                "ERR_ARTIFACT_PATH_NOT_FILE",
                "path must name an output file",
                path,
                false,
                0,
            ));
        }
        let target = target::Target::prepare(requested).map_err(|error| {
            failure(
                "ERR_FILE_TARGET_LEASE_UNAVAILABLE",
                error.to_string(),
                path,
                false,
                0,
            )
        })?;
        let existed = target.path.try_exists().map_err(|error| {
            failure(
                "ERR_FILE_TARGET_LEASE_UNAVAILABLE",
                error.to_string(),
                path,
                false,
                0,
            )
        })?;
        if existed && !overwrite {
            return Err(failure(
                "ERR_ARTIFACT_TARGET_EXISTS",
                "target file already exists",
                path,
                false,
                0,
            ));
        }
        let baseline = if existed {
            let mut guard = EditGuard::acquire(&target.path).map_err(|error| {
                failure(
                    "ERR_FILE_TARGET_LEASE_UNAVAILABLE",
                    format!("could not capture destination: {error:?}"),
                    path,
                    false,
                    0,
                )
            })?;
            let bytes = guard
                .read_bounded(session_storage::MAX_BYTES as u64)
                .map_err(|error| {
                    failure(
                        "ERR_FILE_TARGET_LEASE_UNAVAILABLE",
                        format!("could not snapshot destination: {error:?}"),
                        path,
                        false,
                        0,
                    )
                })?;
            guard
                .validate_current(&target.path, &bytes, session_storage::MAX_BYTES as u64)
                .map_err(|_| {
                    failure(
                        "ERR_FILE_TARGET_LEASE_CHANGED",
                        "destination changed during preparation",
                        path,
                        false,
                        0,
                    )
                })?;
            Some(Baseline { guard, bytes })
        } else {
            None
        };
        target.revalidate().map_err(|error| {
            failure(
                "ERR_FILE_TARGET_LEASE_CHANGED",
                error.to_string(),
                path,
                false,
                0,
            )
        })?;
        Ok(Self {
            artifact,
            text,
            target,
            baseline,
            overwrite,
        })
    }

    pub(in crate::overlay::computer_control) fn proposal(&self) -> Value {
        let mut proposal = self.target.proposal();
        proposal["capability_class"] = json!("dedicated_local_file_write");
        proposal["operation"] = json!(if self.baseline.is_some() {
            "replace_existing_file"
        } else {
            "create_file"
        });
        proposal["target_existed_before"] = json!(self.baseline.is_some());
        proposal["overwrite_requested"] = json!(self.overwrite);
        proposal["source_artifact_id"] = json!(self.artifact.id);
        proposal["source_sha256"] = json!(self.artifact.sha256);
        proposal["source_byte_count"] = json!(self.artifact.bytes);
        if let Some(baseline) = &self.baseline {
            let id = baseline.guard.identity();
            proposal["target_volume"] = json!(id.volume);
            proposal["target_file_id"] = json!(format!("{:032x}", u128::from_le_bytes(id.id)));
            proposal["target_sha256"] = json!(sha256_hex(&baseline.bytes));
        }
        proposal
    }

    pub(in crate::overlay::computer_control) fn commit(mut self, cancel: &AtomicBool) -> Value {
        let mut result = self.commit_inner(cancel);
        result["created_parent_paths"] = json!(self.target.created_paths);
        result
    }

    fn commit_inner(&mut self, cancel: &AtomicBool) -> Value {
        let path = self.target.path.to_string_lossy().to_string();
        // This is a short admission edge. Retirement does not wait for disk I/O;
        // an already admitted explicit export retains its immutable source.
        if !self
            .artifact
            .owner
            .same_session(&super::super::session_retention::current())
            || self.artifact.owner.while_open(|| ()).is_none()
        {
            return failure(
                "ERR_ARTIFACT_SESSION_RETIRED",
                "artifact session has retired",
                &path,
                false,
                0,
            );
        }
        if cancel.load(Ordering::SeqCst) {
            return failure("ERR_CANCELLED", "artifact save cancelled", &path, false, 0);
        }
        if let Err(error) = self.validate() {
            return failure("ERR_FILE_TARGET_LEASE_CHANGED", error, &path, false, 0);
        }
        if let Err(error) = self.target.create_parents(cancel) {
            return failure(
                if cancel.load(Ordering::SeqCst) {
                    "ERR_CANCELLED"
                } else {
                    "ERR_FILE_TARGET_LEASE_CHANGED"
                },
                error.to_string(),
                &path,
                false,
                self.target.created_directories,
            );
        }
        let created = self.target.created_directories;
        if cancel.load(Ordering::SeqCst) {
            return failure(
                "ERR_CANCELLED",
                "artifact save cancelled",
                &path,
                false,
                created,
            );
        }
        let staged = match write_synced_sibling(&self.target.path, self.text.as_bytes()) {
            Ok(staged) => staged,
            Err(error) => {
                let mut result = failure(
                    "ERR_ARTIFACT_SAVE_STAGE",
                    error.to_string(),
                    &path,
                    false,
                    created,
                );
                result["effect_may_have_occurred"] = json!(true);
                result["staging_cleanup_unverified"] = json!(true);
                return result;
            }
        };
        if cancel.load(Ordering::SeqCst) {
            return abort_staged(
                staged,
                failure(
                    "ERR_CANCELLED",
                    "artifact save cancelled",
                    &path,
                    false,
                    created,
                ),
            );
        }
        if let Err(error) = self.validate() {
            return abort_staged(
                staged,
                failure(
                    "ERR_FILE_TARGET_LEASE_CHANGED",
                    error,
                    &path,
                    false,
                    created,
                ),
            );
        }
        let outcome = if let Some(baseline) = self.baseline.take() {
            let current = match baseline.guard.validate_current(
                &self.target.path,
                &baseline.bytes,
                session_storage::MAX_BYTES as u64,
            ) {
                Ok(current) => current,
                Err(_) => {
                    return abort_staged(
                        staged,
                        failure(
                            "ERR_FILE_TARGET_LEASE_CHANGED",
                            "destination changed before commit",
                            &path,
                            false,
                            created,
                        ),
                    );
                }
            };
            baseline.guard.commit_cancellable(
                &self.target.path,
                current,
                staged,
                &baseline.bytes,
                self.text.as_bytes(),
                CommitOptions {
                    max_bytes: session_storage::MAX_BYTES as u64,
                    cancel,
                },
            )
        } else {
            staged.publish_new(
                self.target.parent(),
                &self.target.path,
                self.text.as_bytes(),
                Some(cancel),
            )
        };
        self.receipt(outcome)
    }

    fn validate(&self) -> Result<(), String> {
        self.target
            .revalidate()
            .map_err(|error| error.to_string())?;
        if let Some(baseline) = &self.baseline {
            baseline
                .guard
                .validate_current(
                    &self.target.path,
                    &baseline.bytes,
                    session_storage::MAX_BYTES as u64,
                )
                .map_err(|_| {
                    "destination identity or bytes changed since authorization".to_string()
                })?;
        } else if std::fs::symlink_metadata(&self.target.path).is_ok() {
            return Err("a destination appeared after preparation".into());
        }
        Ok(())
    }

    fn receipt(&self, outcome: CommitOutcome) -> Value {
        let path = self.target.path.to_string_lossy();
        let created = self.target.created_directories;
        match outcome {
            CommitOutcome::NoEffect {
                error,
                retained_staging,
            } => {
                let mut result = failure("ERR_ARTIFACT_SAVE_COMMIT", error, &path, false, created);
                if let Some(staging) = retained_staging {
                    result["retained_staging_path"] = json!(staging);
                    result["effect_may_have_occurred"] = json!(true);
                    result["executed"] = json!(true);
                }
                result
            }
            CommitOutcome::Ambiguous {
                error,
                tool_mutated_file,
                external_change_detected,
                recovery_backup,
                recovery_sha256,
                retained_staging,
            } => {
                let mut result =
                    failure("ERR_ARTIFACT_SAVE_UNVERIFIED", error, &path, true, created);
                result["tool_mutated_file"] = json!(tool_mutated_file);
                result["external_change_detected"] = json!(external_change_detected);
                result["recovery_backup"] = json!(recovery_backup);
                result["recovery_sha256"] = json!(recovery_sha256);
                result["retained_staging_path"] = json!(retained_staging);
                result
            }
            CommitOutcome::Verified { retained_backup } => {
                if let Err(error) = self.target.revalidate() {
                    let mut result = failure(
                        "ERR_ARTIFACT_SAVE_UNVERIFIED",
                        error.to_string(),
                        &path,
                        true,
                        created,
                    );
                    result["retained_backup"] = json!(retained_backup);
                    return result;
                }
                json!({
                    "ok": true, "saved": true, "path": path,
                    "byte_count": self.artifact.bytes, "sha256": self.artifact.sha256,
                    "source_artifact_id": self.artifact.id, "source_sha256": self.artifact.sha256,
                    "effect_verified": true, "effect_may_have_occurred": true, "executed": true,
                    "atomic": true, "created_parent_count": created, "retained_backup": retained_backup,
                    "completion_proof": { "exact": ["/path", "/byte_count", "/sha256", "/source_artifact_id", "/source_sha256"], "partial": [] }
                })
            }
        }
    }
}

fn abort_staged(staged: StagedFile, mut result: Value) -> Value {
    let path = staged.path().to_owned();
    if let Err(error) = staged.discard() {
        result["retained_staging_path"] = json!(path);
        result["staging_cleanup_error"] = json!(error.to_string());
        result["effect_may_have_occurred"] = json!(true);
        result["executed"] = json!(true);
    }
    result
}

fn failure(
    code: &str,
    error: impl Into<String>,
    path: &str,
    target_effect: bool,
    parents: usize,
) -> Value {
    json!({
        "ok": false, "code": code, "error": error.into(), "path": path,
        "effect_verified": false, "target_effect_may_have_occurred": target_effect,
        "effect_may_have_occurred": target_effect || parents > 0,
        "executed": target_effect || parents > 0, "retryable": true,
        "created_parent_count": parents,
    })
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
