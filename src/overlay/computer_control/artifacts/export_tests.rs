use super::*;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "sgt-artifact-export-{:032x}",
            u128::from_le_bytes(nonce)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> PathBuf {
        self.0.join("output.txt")
    }
    fn prepare(&self, overwrite: bool) -> PreparedSave {
        let artifact = super::super::create_text("test", None, None, "new complete bytes").unwrap();
        PreparedSave::prepare(&artifact.id, self.path().to_str().unwrap(), overwrite)
            .unwrap_or_else(|error| panic!("{error}"))
    }
    fn entries(&self) -> usize {
        std::fs::read_dir(&self.0).unwrap().count()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Each fixture exclusively creates this unpredictable root. Remove only
        // its direct test-created files and known empty test directory.
        for entry in std::fs::read_dir(&self.0).unwrap() {
            let path = entry.unwrap().path();
            assert!(path.parent() == Some(self.0.as_path()));
            if path.is_file() {
                std::fs::remove_file(path).unwrap();
            } else {
                assert!(matches!(
                    path.file_name().unwrap().to_str(),
                    Some("new-parent" | "moved-parent")
                ));
                let output = path.join("output.txt");
                if output.exists() {
                    std::fs::remove_file(output).unwrap();
                }
                std::fs::remove_dir(path).unwrap();
            }
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn artifact_export_publication_is_atomic_for_create_and_overwrite() {
    let _session = super::super::super::session_retention::start_session();
    let fixture = Fixture::new();
    for overwrite in [false, true] {
        let prepared = fixture.prepare(overwrite);
        let proposal = prepared.proposal();
        assert_eq!(proposal["target_existed_before"], overwrite);
        assert!(!proposal.to_string().contains("new complete bytes"));
        let result = prepared.commit(&AtomicBool::new(false));
        assert_eq!(result["effect_verified"], true, "{result}");
        assert_eq!(result["atomic"], true);
        assert_eq!(
            std::fs::read(fixture.path()).unwrap(),
            b"new complete bytes"
        );
        assert_eq!(fixture.entries(), 1);
    }
}

#[test]
fn artifact_export_missing_target_never_replaces_a_concurrent_creator() {
    let _session = super::super::super::session_retention::start_session();
    for overwrite in [false, true] {
        let fixture = Fixture::new();
        let prepared = fixture.prepare(overwrite);
        std::fs::write(fixture.path(), b"competing bytes").unwrap();
        let result = prepared.commit(&AtomicBool::new(false));
        assert_eq!(result["code"], "ERR_FILE_TARGET_LEASE_CHANGED", "{result}");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(std::fs::read(fixture.path()).unwrap(), b"competing bytes");
        assert_eq!(fixture.entries(), 1);
    }
}

#[test]
fn artifact_export_revalidates_both_existing_identity_and_bytes() {
    let _session = super::super::super::session_retention::start_session();
    for replace_identity in [false, true] {
        let fixture = Fixture::new();
        std::fs::write(fixture.path(), b"original bytes").unwrap();
        let prepared = fixture.prepare(true);
        let expected = if replace_identity {
            std::fs::rename(fixture.path(), fixture.0.join("original.txt")).unwrap();
            std::fs::write(fixture.path(), b"original bytes").unwrap();
            b"original bytes".as_slice()
        } else {
            std::fs::write(fixture.path(), b"changed bytes").unwrap();
            b"changed bytes".as_slice()
        };
        let result = prepared.commit(&AtomicBool::new(false));
        assert_eq!(result["code"], "ERR_FILE_TARGET_LEASE_CHANGED", "{result}");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(std::fs::read(fixture.path()).unwrap(), expected);
    }
}

#[test]
fn artifact_export_cancellation_and_retirement_prevent_new_writes() {
    for retire in [false, true] {
        let session = super::super::super::session_retention::start_session();
        let fixture = Fixture::new();
        let prepared = fixture.prepare(false);
        let result = if retire {
            let owner = prepared.artifact.owner.clone();
            drop(session);
            let _retired_scope = owner.enter();
            prepared.commit(&AtomicBool::new(false))
        } else {
            prepared.commit(&AtomicBool::new(true))
        };
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert_eq!(fixture.entries(), 0);
    }
}

#[test]
fn artifact_export_commit_failure_preserves_original_bytes() {
    let _session = super::super::super::session_retention::start_session();
    let fixture = Fixture::new();
    std::fs::write(fixture.path(), b"original bytes").unwrap();
    let prepared = fixture.prepare(true);
    let blocker = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(fixture.path())
        .unwrap();
    let result = prepared.commit(&AtomicBool::new(false));
    assert_eq!(result["code"], "ERR_ARTIFACT_SAVE_COMMIT", "{result}");
    assert_eq!(result["target_effect_may_have_occurred"], false);
    assert_eq!(std::fs::read(fixture.path()).unwrap(), b"original bytes");
    drop(blocker);
    assert_eq!(fixture.entries(), 1);
}

#[test]
fn artifact_export_native_create_only_closes_the_final_namespace_race() {
    let fixture = Fixture::new();
    let target = target::Target::prepare(&fixture.path()).unwrap();
    let staged = write_synced_sibling(&target.path, b"staged bytes").unwrap();
    std::fs::write(fixture.path(), b"concurrent winner").unwrap();
    assert!(matches!(
        staged.publish_new(target.parent(), &target.path, b"staged bytes", None),
        CommitOutcome::NoEffect { .. }
    ));
    assert_eq!(std::fs::read(fixture.path()).unwrap(), b"concurrent winner");
    assert_eq!(fixture.entries(), 1);
}

#[test]
fn artifact_export_late_overwrite_race_retains_competing_bytes_for_recovery() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path(), b"original").unwrap();
    let mut guard = EditGuard::acquire(&fixture.path()).unwrap();
    let original = guard.read_bounded(100).unwrap();
    let current = guard
        .validate_current(&fixture.path(), &original, 100)
        .unwrap();
    let staged = write_synced_sibling(&fixture.path(), b"export").unwrap();
    std::fs::write(fixture.path(), b"competitor").unwrap();
    match guard.commit_audited(&fixture.path(), current, staged, &original, b"export", 100) {
        CommitOutcome::Ambiguous {
            recovery_backup: Some(backup),
            recovery_sha256,
            ..
        } => {
            assert_eq!(std::fs::read(backup).unwrap(), b"competitor");
            assert_eq!(recovery_sha256.unwrap(), sha256_hex(b"competitor"));
        }
        _ => panic!("late changed version must not be verified"),
    }
    assert_eq!(std::fs::read(fixture.path()).unwrap(), b"export");
}

#[test]
fn artifact_export_failed_readback_retains_original_recovery_without_claiming_success() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path(), b"old").unwrap();
    let mut guard = EditGuard::acquire(&fixture.path()).unwrap();
    let original = guard.read_bounded(100).unwrap();
    let current = guard
        .validate_current(&fixture.path(), &original, 100)
        .unwrap();
    let staged = write_synced_sibling(&fixture.path(), b"longer export").unwrap();
    match guard.commit_audited(
        &fixture.path(),
        current,
        staged,
        &original,
        b"longer export",
        3,
    ) {
        CommitOutcome::Ambiguous {
            recovery_backup: Some(backup),
            ..
        } => {
            assert_eq!(std::fs::read(backup).unwrap(), b"old");
        }
        _ => panic!("failed bounded readback must remain uncertain"),
    }
}

#[test]
fn artifact_export_preparation_has_no_parent_creation_effect() {
    let _session = super::super::super::session_retention::start_session();
    let fixture = Fixture::new();
    let artifact = super::super::create_text("test", None, None, "payload").unwrap();
    let output = fixture.0.join("new-parent").join("output.txt");
    let prepared = PreparedSave::prepare(&artifact.id, output.to_str().unwrap(), false)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fixture.entries(), 0);
    let result = prepared.commit(&AtomicBool::new(true));
    assert_eq!(result["effect_may_have_occurred"], false);
    assert_eq!(fixture.entries(), 0);
}

#[test]
fn artifact_export_new_parent_receipt_and_complete_bytes_agree() {
    let _session = super::super::super::session_retention::start_session();
    let fixture = Fixture::new();
    let artifact = super::super::create_text("test", None, None, "payload").unwrap();
    let output = fixture.0.join("new-parent").join("output.txt");
    let result = PreparedSave::prepare(&artifact.id, output.to_str().unwrap(), false)
        .unwrap_or_else(|error| panic!("{error}"))
        .commit(&AtomicBool::new(false));
    assert_eq!(result["effect_verified"], true, "{result}");
    assert_eq!(result["created_parent_count"], 1);
    assert_eq!(result["created_parent_paths"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(&output).unwrap(), b"payload");
    std::fs::remove_file(output).unwrap();
}

#[test]
fn artifact_export_staging_refuses_non_directory_parent_without_changing_target() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path(), b"original").unwrap();
    assert!(write_synced_sibling(&fixture.path().join("child"), b"payload").is_err());
    assert_eq!(std::fs::read(fixture.path()).unwrap(), b"original");
    assert_eq!(fixture.entries(), 1);
}

#[test]
fn artifact_export_concurrent_publications_have_one_complete_winner() {
    let fixture = Fixture::new();
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let handles = [b"first".as_slice(), b"second".as_slice()].map(|bytes| {
            let barrier = &barrier;
            let path = fixture.path();
            scope.spawn(move || {
                let target = target::Target::prepare(&path).unwrap();
                let staged = write_synced_sibling(&target.path, bytes).unwrap();
                barrier.wait();
                staged.publish_new(target.parent(), &target.path, bytes, None)
            })
        });
        handles.map(|handle| handle.join().unwrap())
    });
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CommitOutcome::Verified { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                CommitOutcome::NoEffect {
                    retained_staging: None,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(matches!(
        std::fs::read(fixture.path()).unwrap().as_slice(),
        b"first" | b"second"
    ));
    assert_eq!(fixture.entries(), 1);
}

#[test]
fn artifact_export_cancelled_commit_admission_prevents_create_and_replace() {
    for existed in [false, true] {
        let fixture = Fixture::new();
        let target = target::Target::prepare(&fixture.path()).unwrap();
        if existed {
            std::fs::write(&target.path, b"original").unwrap();
        }
        let staged = write_synced_sibling(&target.path, b"export").unwrap();
        let cancel = AtomicBool::new(true);
        let outcome = if existed {
            let mut guard = EditGuard::acquire(&target.path).unwrap();
            let bytes = guard.read_bounded(100).unwrap();
            let current = guard.validate_current(&target.path, &bytes, 100).unwrap();
            guard.commit_cancellable(
                &target.path,
                current,
                staged,
                &bytes,
                b"export",
                CommitOptions {
                    max_bytes: 100,
                    cancel: &cancel,
                },
            )
        } else {
            staged.publish_new(target.parent(), &target.path, b"export", Some(&cancel))
        };
        assert!(matches!(
            outcome,
            CommitOutcome::NoEffect {
                retained_staging: None,
                ..
            }
        ));
        if existed {
            assert_eq!(std::fs::read(&target.path).unwrap(), b"original");
        }
        assert_eq!(fixture.entries(), usize::from(existed));
    }
}

#[test]
fn artifact_export_failed_staging_cleanup_is_reported_as_a_retained_effect() {
    use windows::Win32::Storage::FileSystem::{DELETE, FILE_SHARE_DELETE, FILE_SHARE_READ};
    let _session = super::super::super::session_retention::start_session();
    let fixture = Fixture::new();
    let prepared = fixture.prepare(false);
    let staged = write_synced_sibling(&prepared.target.path, b"export").unwrap();
    let staging = staged.path().to_owned();
    let blocker = OpenOptions::new()
        .access_mode(DELETE.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
        .open(&staging)
        .unwrap();
    std::fs::write(fixture.path(), b"winner").unwrap();
    let outcome = staged.publish_new(
        prepared.target.parent(),
        &prepared.target.path,
        b"export",
        None,
    );
    let result = prepared.receipt(outcome);
    assert_eq!(result["effect_may_have_occurred"], true, "{result}");
    assert_eq!(result["target_effect_may_have_occurred"], false);
    assert_eq!(
        result["retained_staging_path"],
        staging.to_string_lossy().as_ref()
    );
    assert_eq!(std::fs::read(fixture.path()).unwrap(), b"winner");
    drop(blocker);
    assert_eq!(std::fs::read(staging).unwrap(), b"export");
}

#[test]
fn artifact_export_shared_contract_covers_prepared_publication_and_recovery() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/file-mutation-contract.json"
    )))
    .unwrap();
    for name in [
        "artifactSourceAndTargetPreparedBeforeTargetScopeCheck",
        "artifactPreparationHasNoOutputFilesystemEffects",
        "initiallyAbsentTargetStaysCreateOnlyWithOverwriteTrue",
        "artifactExistingTargetLeaseIncludesObjectIdentityAndDigest",
        "artifactStageFailurePreservesOriginalTarget",
        "unverifiedPublicationCannotClaimSuccessOrAutomaticallyRollback",
        "retainedRecoveryAndCreatedParentEffectsAreReported",
        "cancellationPreventsPublicationAfterCommitAdmissionCloses",
    ] {
        assert_eq!(fixture["invariants"][name], true, "{name}");
    }
    let late_race = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["operation"] == "save_artifact_commit_race")
        .unwrap();
    assert_eq!(late_race["expect"]["effectVerified"], false);
    assert_eq!(late_race["expect"]["competingBytesRecoverable"], true);
}

#[test]
fn artifact_export_partial_stage_failure_removes_owned_bytes_and_preserves_original() {
    use std::io::Write;
    let fixture = Fixture::new();
    std::fs::write(fixture.path(), b"original").unwrap();
    let result = crate::overlay::computer_control::executor::file_transaction::write_sibling_with(
        &fixture.path(),
        b"replacement",
        |file, bytes| {
            file.write_all(&bytes[..3])?;
            Err(std::io::Error::other("injected write failure"))
        },
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(fixture.path()).unwrap(), b"original");
    assert_eq!(fixture.entries(), 1);
}

#[test]
fn artifact_export_parent_lease_keeps_directory_operations_available() {
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    };
    let fixture = Fixture::new();
    let open_writer = || {
        OpenOptions::new()
            .access_mode(FILE_GENERIC_WRITE.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(&fixture.0)
    };
    drop(open_writer().unwrap());
    let target = target::Target::prepare(&fixture.path()).unwrap();
    drop(open_writer().unwrap());
    drop(target);
    drop(open_writer().unwrap());
}

#[test]
fn artifact_export_same_path_parent_replacement_invalidates_the_lease() {
    let _session = super::super::super::session_retention::start_session();
    let fixture = Fixture::new();
    let parent = fixture.0.join("new-parent");
    let moved = fixture.0.join("moved-parent");
    std::fs::create_dir(&parent).unwrap();
    let artifact = super::super::create_text("test", None, None, "payload").unwrap();
    let output = parent.join("output.txt");
    let prepared = PreparedSave::prepare(&artifact.id, output.to_str().unwrap(), false)
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::rename(&parent, &moved).unwrap();
    std::fs::create_dir(&parent).unwrap();
    let result = prepared.commit(&AtomicBool::new(false));
    assert_eq!(result["code"], "ERR_FILE_TARGET_LEASE_CHANGED", "{result}");
    assert_eq!(result["effect_may_have_occurred"], false);
    assert!(!output.exists());
    assert!(!moved.join("output.txt").exists());
}
