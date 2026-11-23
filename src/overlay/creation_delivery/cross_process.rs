//! Isolated multi-process delivery and cancellation serialization tests.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::*;

const ROLE: &str = "SGT_TEST_DELIVERY_ROLE";
const SIGNAL_DIR: &str = "SGT_TEST_DELIVERY_SIGNALS";
const TEST_NAME: &str = "overlay::creation_delivery::cross_process::commit_and_cancellation_preserve_distinct_work_across_processes";

pub(super) fn pause_after_load() -> Result<(), String> {
    let Some(directory) = std::env::var_os(SIGNAL_DIR).map(PathBuf::from) else {
        return Ok(());
    };
    std::fs::write(directory.join("cancel-locked"), b"ready").map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !directory.join("release-cancel").exists() {
        if Instant::now() >= deadline {
            return Err("test delivery barrier timed out".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn child(root: &Path, role: &str, signals: bool) -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(TEST_NAME)
        .arg("--nocapture")
        .env(crate::paths::RUNTIME_STATE_ROOT_ENV, root)
        .env(ROLE, role)
        .env_remove(SIGNAL_DIR)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if signals {
        command.env(SIGNAL_DIR, root);
    }
    command.spawn().unwrap()
}

fn wait_success(child: Child) {
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(Instant::now() < deadline, "test synchronization timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assigned(root: &Path, role: &str) -> (String, String, String, PathBuf, PathBuf) {
    let job_id = format!("image_job_{role}");
    let dispatch_id = format!("image_dispatch_{role}");
    let name = format!("result-{dispatch_id}.png");
    let final_dir = root.join("outputs");
    std::fs::create_dir_all(&final_dir).unwrap();
    let final_dir = std::fs::canonicalize(final_dir).unwrap();
    let staging_path = crate::overlay::creation_output::staging_path(&dispatch_id, &name).unwrap();
    (job_id, dispatch_id, name, staging_path, final_dir)
}

fn setup(root: &Path) {
    for role in ["commit", "cancel"] {
        let (job, dispatch, name, staging_path, output) = assigned(root, role);
        let staging = crate::overlay::creation_output::prepare_staging(&dispatch, &name).unwrap();
        std::fs::write(&staging_path, format!("result-{role}")).unwrap();
        let arguments = serde_json::json!({
            "outputName": name,
            "outputDir": staging.directory().to_string_lossy(),
            "finalOutputDir": output.to_string_lossy(),
        });
        crate::overlay::creation_intent_journal::record("image", &job, &dispatch, arguments)
            .unwrap();
        staging.persist();
    }
}

fn fingerprint(dispatch: &str) -> String {
    crate::overlay::creation_intent_journal::load_all()
        .unwrap()
        .into_iter()
        .find(|intent| intent.dispatch_id == dispatch)
        .unwrap()
        .arguments_fingerprint
}

fn commit_one(root: &Path) {
    let (job, dispatch, name, staging, output) = assigned(root, "commit");
    let digest = fingerprint(&dispatch);
    std::fs::write(root.join("commit-attempted"), b"ready").unwrap();
    commit(PublishedDelivery {
        product: "image",
        job_id: job,
        dispatch_id: dispatch,
        request_fingerprint: digest,
        source_path: String::new(),
        output_name: name.clone(),
        staging_path: staging.to_string_lossy().to_string(),
        output_path: output.join(&name).to_string_lossy().to_string(),
        companion: None,
        metadata: serde_json::json!({"operation":"create_image"}),
    })
    .unwrap();
}

fn cancel_one(root: &Path) {
    let (job, dispatch, name, _, _) = assigned(root, "cancel");
    let digest = fingerprint(&dispatch);
    let outcome = cancel_dispatch(CancelledDelivery {
        product: "image",
        job_id: job,
        dispatch_id: dispatch,
        request_fingerprint: digest,
        output_name: name,
    })
    .unwrap();
    assert_eq!(outcome, CancellationOutcome::Cancelled);
}

fn verify(root: &Path) {
    let (_, committed_dispatch, name, _, output) = assigned(root, "commit");
    assert_eq!(std::fs::read(output.join(&name)).unwrap(), b"result-commit");
    assert!(
        crate::overlay::generation_history::has_delivery_receipt(
            "image",
            &committed_dispatch,
            &output.join(&name).to_string_lossy()
        )
        .unwrap()
    );
    assert!(
        crate::overlay::creation_intent_journal::load_all()
            .unwrap()
            .is_empty()
    );
    let (_, cancel_dispatch, cancel_name, _, _) = assigned(root, "cancel");
    assert!(
        !crate::overlay::creation_output::staging_path(&cancel_dispatch, &cancel_name)
            .unwrap()
            .exists()
    );
    assert!(
        load_store(&journal_path())
            .unwrap()
            .cancellations
            .is_empty()
    );
}

#[test]
fn commit_and_cancellation_preserve_distinct_work_across_processes() {
    if let Ok(role) = std::env::var(ROLE) {
        let root = PathBuf::from(std::env::var_os(crate::paths::RUNTIME_STATE_ROOT_ENV).unwrap());
        match role.as_str() {
            "setup" => setup(&root),
            "commit" => commit_one(&root),
            "cancel" => cancel_one(&root),
            "verify" => verify(&root),
            "published" => preserve_published(&root),
            _ => panic!("unknown test role"),
        }
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "sgt-delivery-process-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(root.starts_with(&temp) && root != temp);
    wait_success(child(&root, "setup", false));
    let cancellation = child(&root, "cancel", true);
    wait_for(&root.join("cancel-locked"));
    let mut committed = child(&root, "commit", false);
    wait_for(&root.join("commit-attempted"));
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        committed.try_wait().unwrap().is_none(),
        "commit must wait for cancellation ownership"
    );
    std::fs::write(root.join("release-cancel"), b"continue").unwrap();
    wait_success(cancellation);
    wait_success(committed);
    wait_success(child(&root, "verify", false));
    std::fs::remove_dir_all(root).unwrap();
}

fn preserve_published(root: &Path) {
    setup(root);
    let (job, dispatch, name, staging, output_dir) = assigned(root, "commit");
    let digest = fingerprint(&dispatch);
    let artifact =
        crate::overlay::generation_history::inspect_delivery_artifact(&staging.to_string_lossy())
            .unwrap();
    let output = output_dir.join(&name);
    let mut record = DeliveryRecord {
        product: "image".into(),
        job_id: job.clone(),
        dispatch_id: dispatch.clone(),
        request_fingerprint: digest.clone(),
        source_path: String::new(),
        output_name: name.clone(),
        staging_path: staging.to_string_lossy().into_owned(),
        output_path: output.to_string_lossy().into_owned(),
        publication_path: publication::reserve_path(&output, &dispatch)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        publication_claim: publication::new_claim().unwrap(),
        publication_file_identity: None,
        metadata: serde_json::json!({"operation":"create_image"}),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        companion: None,
        stage: DeliveryStage::Validated,
    };
    let receipt = publication::create_receipt(&record).unwrap();
    record.publication_file_identity = Some(receipt.identity().to_string());
    drop(receipt);
    publication::prepare(&record).unwrap();
    record.stage = DeliveryStage::PublicationPrepared;
    let _published = publication::publish_prepared(&record).unwrap();
    record.stage = DeliveryStage::Published;
    let store = DeliveryStore {
        version: VERSION,
        entries: vec![record],
        cancellations: vec![],
    };
    save_store(&journal_path(), &store).unwrap();
    assert_eq!(
        cancel_dispatch(CancelledDelivery {
            product: "image",
            job_id: job.clone(),
            dispatch_id: dispatch.clone(),
            request_fingerprint: digest,
            output_name: name,
        })
        .unwrap(),
        CancellationOutcome::Published
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"result-commit");
    assert!(
        crate::overlay::generation_history::has_delivery_receipt(
            "image",
            &dispatch,
            &output.to_string_lossy()
        )
        .unwrap()
    );
    let store = load_store(&journal_path()).unwrap();
    assert!(store.cancellations.is_empty());
    assert!(store.entries.is_empty());
}

#[test]
fn a_published_result_wins_a_later_cancel_without_losing_history() {
    let root = std::env::temp_dir().join(format!(
        "sgt-delivery-published-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(root.starts_with(&temp) && root != temp);
    wait_success(child(&root, "published", false));
    std::fs::remove_dir_all(root).unwrap();
}
