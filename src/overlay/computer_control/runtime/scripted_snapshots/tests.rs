use super::*;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sgt-scripted-snapshot-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let temp = std::env::temp_dir();
        assert_eq!(self.0.parent(), Some(temp.as_path()));
        assert!(
            self.0
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("sgt-scripted-snapshot-"))
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn snapshot_is_exact_and_raced_destination_file_is_never_replaced() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let root = temp.0.join("captures");
    let content = b"first\0second\r\nthird";
    std::fs::write(&source, content).unwrap();
    let snapshots = ScriptedSnapshots::for_test(vec![source], root.clone()).unwrap();

    snapshots.capture_turn(1).unwrap();

    let captured = root.join("turn-0001/file-0001.snapshot");
    assert_eq!(std::fs::read(&captured).unwrap(), content);
    assert_eq!(
        format!("{:x}", Sha256::digest(content)),
        format!("{:x}", Sha256::digest(std::fs::read(captured).unwrap()))
    );

    std::fs::rename(root.join("turn-0001"), root.join("retained-turn")).unwrap();
    std::fs::write(root.join("turn-0001"), b"protected destination").unwrap();
    assert!(snapshots.capture_turn(1).is_err());
    assert_eq!(
        std::fs::read(root.join("turn-0001")).unwrap(),
        b"protected destination"
    );
    assert_eq!(
        std::fs::read(root.join("retained-turn/file-0001.snapshot")).unwrap(),
        content
    );
}

#[test]
fn stable_replacement_before_capture_becomes_the_bound_source() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let root = temp.0.join("captures");
    std::fs::write(&source, b"old identity").unwrap();
    let snapshots = ScriptedSnapshots::for_test(vec![source.clone()], root.clone()).unwrap();

    replace_at_path(&temp.0, &source, b"new stable identity", "before");
    snapshots.capture_turn(1).unwrap();

    assert_eq!(
        std::fs::read(root.join("turn-0001/file-0001.snapshot")).unwrap(),
        b"new stable identity"
    );
}

#[test]
fn source_replacement_or_mutation_between_probes_is_rejected() {
    let temp = TestDir::new();
    let replaced = temp.0.join("replaced.bin");
    let mutated = temp.0.join("mutated.bin");
    std::fs::write(&replaced, b"first identity").unwrap();
    std::fs::write(&mutated, b"alpha").unwrap();
    let configured = vec![
        ConfiguredSource::configure(replaced.clone()).unwrap(),
        ConfiguredSource::configure(mutated.clone()).unwrap(),
    ];
    let baselines = probe_sources(&configured).unwrap();

    replace_at_path(&temp.0, &replaced, b"second identity", "between");
    std::fs::write(&mutated, b"bravo").unwrap();
    let error = LockedSource::acquire_all(&configured, &baselines)
        .err()
        .unwrap();

    assert!(matches!(
        error.reason,
        "source_identity_changed" | "source_metadata_changed"
    ));
}

#[test]
fn source_content_mutation_between_probes_is_rejected() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    std::fs::write(&source, b"alpha").unwrap();
    let configured = vec![ConfiguredSource::configure(source.clone()).unwrap()];
    let baselines = probe_sources(&configured).unwrap();

    std::fs::write(&source, b"bravo").unwrap();
    let error = LockedSource::acquire_all(&configured, &baselines)
        .err()
        .unwrap();

    assert!(matches!(
        error.reason,
        "source_metadata_changed" | "source_content_changed"
    ));
}

#[test]
fn complete_source_set_stays_locked_between_sequential_authentication() {
    let temp = TestDir::new();
    let first = temp.0.join("first.bin");
    let second = temp.0.join("second.bin");
    let root = temp.0.join("captures");
    std::fs::write(&first, b"one").unwrap();
    std::fs::write(&second, b"two").unwrap();
    std::fs::create_dir(&root).unwrap();
    let configured = vec![
        ConfiguredSource::configure(first.clone()).unwrap(),
        ConfiguredSource::configure(second.clone()).unwrap(),
    ];
    let baselines = probe_sources(&configured).unwrap();
    let mut locked = LockedSource::acquire_all(&configured, &baselines).unwrap();
    let staging = StagingTurn::new(&root).unwrap();
    let mut artifacts = Vec::new();
    let mut evidence = Vec::new();
    for (offset, source) in locked.iter_mut().enumerate() {
        let (artifact, proof) = source.stage(&staging.artifact_path(offset + 1)).unwrap();
        artifacts.push(artifact);
        evidence.push(proof);
    }

    locked[0].authenticate(&evidence[0]).unwrap();
    assert!(std::fs::write(&first, b"changed").is_err());
    let replacement = temp.0.join("replacement-locked.bin");
    std::fs::write(&replacement, b"replacement").unwrap();
    assert!(std::fs::rename(&replacement, &second).is_err());
    locked[1].authenticate(&evidence[1]).unwrap();

    let destination = root.join("turn-0001");
    staging.publish(artifacts, &destination).unwrap();
    assert_eq!(
        std::fs::read(destination.join("file-0001.snapshot")).unwrap(),
        b"one"
    );
    assert_eq!(
        std::fs::read(destination.join("file-0002.snapshot")).unwrap(),
        b"two"
    );
}

#[test]
fn retained_stage_handles_block_mutation_and_inventory_rejects_extras() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let root = temp.0.join("captures");
    std::fs::write(&source, b"trusted").unwrap();
    std::fs::create_dir(&root).unwrap();
    let configured = vec![ConfiguredSource::configure(source).unwrap()];
    let baselines = probe_sources(&configured).unwrap();
    let mut locked = LockedSource::acquire_all(&configured, &baselines).unwrap();
    let staging = StagingTurn::new(&root).unwrap();
    let staged_path = staging.artifact_path(1);
    let (artifact, _) = locked[0].stage(&staged_path).unwrap();
    let mut artifacts = vec![artifact];

    assert!(OpenOptions::new().write(true).open(&staged_path).is_err());
    let replacement = staging.path().join("replacement.bin");
    std::fs::write(&replacement, b"hostile").unwrap();
    assert!(std::fs::rename(&replacement, &staged_path).is_err());
    std::fs::remove_file(&replacement).unwrap();
    staging.authenticate_held(&mut artifacts).unwrap();

    std::fs::write(staging.path().join("unexpected.bin"), b"extra").unwrap();
    let error = staging.authenticate_held(&mut artifacts).unwrap_err();
    assert_eq!(error.reason, "staged_inventory_mismatch");
}

#[test]
fn post_publish_authentication_detects_close_to_rename_tampering() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let root = temp.0.join("captures");
    std::fs::write(&source, b"trusted").unwrap();
    std::fs::create_dir(&root).unwrap();
    let configured = vec![ConfiguredSource::configure(source).unwrap()];
    let baselines = probe_sources(&configured).unwrap();
    let mut locked = LockedSource::acquire_all(&configured, &baselines).unwrap();
    let staging = StagingTurn::new(&root).unwrap();
    let (artifact, _) = locked[0].stage(&staging.artifact_path(1)).unwrap();
    let mut artifacts = vec![artifact];
    staging.authenticate_held(&mut artifacts).unwrap();
    let proof = artifacts[0].proof().unwrap();
    drop(artifacts);
    let destination = root.join("turn-0001");
    publication::move_no_replace(staging.path(), &destination).unwrap();

    std::fs::write(destination.join("file-0001.snapshot"), b"hostile").unwrap();
    let error = publication::authenticate_published(
        &destination,
        &[(OsString::from("file-0001.snapshot"), proof)],
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        "published_metadata_changed" | "published_content_changed"
    ));
}

#[test]
fn post_publish_authentication_detects_close_to_rename_replacement() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let root = temp.0.join("captures");
    std::fs::write(&source, b"trusted").unwrap();
    std::fs::create_dir(&root).unwrap();
    let configured = vec![ConfiguredSource::configure(source).unwrap()];
    let baselines = probe_sources(&configured).unwrap();
    let mut locked = LockedSource::acquire_all(&configured, &baselines).unwrap();
    let staging = StagingTurn::new(&root).unwrap();
    let (artifact, _) = locked[0].stage(&staging.artifact_path(1)).unwrap();
    let mut artifacts = vec![artifact];
    staging.authenticate_held(&mut artifacts).unwrap();
    let proof = artifacts[0].proof().unwrap();
    drop(artifacts);
    let destination = root.join("turn-0001");
    publication::move_no_replace(staging.path(), &destination).unwrap();

    let published = destination.join("file-0001.snapshot");
    let retired = root.join("retired.snapshot");
    let replacement = destination.join("replacement.snapshot");
    std::fs::write(&replacement, b"trusted").unwrap();
    std::fs::rename(&published, &retired).unwrap();
    std::fs::rename(&replacement, &published).unwrap();
    let error = publication::authenticate_published(
        &destination,
        &[(OsString::from("file-0001.snapshot"), proof)],
    )
    .err()
    .unwrap();
    assert_eq!(error.reason, "published_identity_changed");
}

#[test]
fn move_file_publication_never_clobbers_file_or_directory() {
    let temp = TestDir::new();

    let staged_for_file = temp.0.join("staged-for-file");
    let destination_file = temp.0.join("turn-file");
    std::fs::create_dir(&staged_for_file).unwrap();
    std::fs::write(staged_for_file.join("payload"), b"staged").unwrap();
    std::fs::write(&destination_file, b"protected file").unwrap();
    let error = publication::move_no_replace(&staged_for_file, &destination_file).unwrap_err();
    assert_eq!(error.reason, "publication_destination_exists");
    assert!(staged_for_file.exists());
    assert_eq!(std::fs::read(destination_file).unwrap(), b"protected file");

    let staged_for_dir = temp.0.join("staged-for-dir");
    let destination_dir = temp.0.join("turn-dir");
    std::fs::create_dir(&staged_for_dir).unwrap();
    std::fs::write(staged_for_dir.join("payload"), b"staged").unwrap();
    std::fs::create_dir(&destination_dir).unwrap();
    std::fs::write(destination_dir.join("protected"), b"directory").unwrap();
    let error = publication::move_no_replace(&staged_for_dir, &destination_dir).unwrap_err();
    assert_eq!(error.reason, "publication_destination_exists");
    assert!(staged_for_dir.exists());
    assert_eq!(
        std::fs::read(destination_dir.join("protected")).unwrap(),
        b"directory"
    );
}

#[test]
fn failed_staging_is_orphaned_and_drop_never_deletes_a_replacement() {
    let temp = TestDir::new();
    let root = temp.0.join("captures");
    std::fs::create_dir(&root).unwrap();
    let staging = StagingTurn::new(&root).unwrap();
    let reserved = staging.path().to_path_buf();
    let retired = root.join("retired-orphan");
    std::fs::rename(&reserved, &retired).unwrap();
    std::fs::create_dir(&reserved).unwrap();
    std::fs::write(reserved.join("protected"), b"replacement object").unwrap();

    drop(staging);

    assert_eq!(
        std::fs::read(reserved.join("protected")).unwrap(),
        b"replacement object"
    );
    assert!(retired.exists());
}

#[test]
fn cryptographic_staging_names_are_distinct_and_not_turn_derived() {
    let temp = TestDir::new();
    let first = StagingTurn::new(&temp.0).unwrap();
    let second = StagingTurn::new(&temp.0).unwrap();
    let first_name = first.path().file_name().unwrap().to_string_lossy();
    let second_name = second.path().file_name().unwrap().to_string_lossy();
    let first_nonce = first_name.strip_prefix(".snapshot-stage-").unwrap();
    assert_eq!(first_nonce.len(), 32);
    assert!(first_nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(first_name, second_name);
}

#[test]
fn root_lock_pins_the_configured_hierarchy_for_the_driver_lifetime() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let root = temp.0.join("captures");
    std::fs::write(&source, b"state").unwrap();
    let snapshots = ScriptedSnapshots::for_test(vec![source], root.clone()).unwrap();

    assert!(std::fs::rename(&root, temp.0.join("moved-root")).is_err());
    snapshots.capture_turn(1).unwrap();
    assert_eq!(
        std::fs::read(root.join("turn-0001/file-0001.snapshot")).unwrap(),
        b"state"
    );
}

#[test]
fn configuration_requires_both_values_and_new_absolute_paths() {
    let temp = TestDir::new();
    let root = temp.0.join("captures");
    assert!(ScriptedSnapshots::from_values(Some(r#"["relative"]"#.into()), None).is_err());
    assert!(
        ScriptedSnapshots::from_values(Some(r#"["relative"]"#.into()), Some(root.clone().into()))
            .is_err()
    );
    assert!(ScriptedSnapshots::from_values(Some("[]".into()), Some(root.clone().into())).is_err());

    let source = temp.0.join("source.bin");
    std::fs::write(&source, b"state").unwrap();
    std::fs::create_dir(&root).unwrap();
    let paths_json = serde_json::to_string(&vec![source]).unwrap();
    assert!(ScriptedSnapshots::from_values(Some(paths_json.into()), Some(root.into())).is_err());
    assert!(
        ScriptedSnapshots::from_values(None, None)
            .unwrap()
            .is_none()
    );
}

fn replace_at_path(root: &Path, path: &Path, content: &[u8], label: &str) {
    let replacement = root.join(format!("replacement-{label}.bin"));
    let retired = root.join(format!("retired-{label}.bin"));
    std::fs::write(&replacement, content).unwrap();
    std::fs::rename(path, retired).unwrap();
    std::fs::rename(replacement, path).unwrap();
}
