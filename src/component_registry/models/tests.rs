use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use sha2::{Digest as _, Sha256};

use super::*;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial_test() -> MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|value| value.into_inner())
}

fn unique_id(label: &str) -> String {
    format!(
        "test-model-{label}-{}-{}",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn model_file(path: &str, bytes: &[u8]) -> ModelFile {
    ModelFile {
        path: path.into(),
        url: None,
        size_bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}

fn synthetic_delivery(id: String, files: Vec<ModelFile>) -> ModelDelivery {
    ModelDelivery {
        id,
        version: "unit".to_string(),
        architecture: ARCHITECTURE.to_string(),
        archive: None,
        installed_size_bytes: files.iter().map(|file| file.size_bytes).sum(),
        files,
        legacy_root: None,
    }
}

fn write_install(delivery: &ModelDelivery, payloads: &[&[u8]]) -> PathBuf {
    let _mutation = super::super::acquire_mutation_guard().unwrap();
    assert_eq!(delivery.files.len(), payloads.len());
    let root = super::super::ensure_version_root(&delivery.id, &delivery.version).unwrap();
    for (file, bytes) in delivery.files.iter().zip(payloads) {
        let path = staging::prepare_target(&root, &file.path).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    super::super::write_receipt(
        &root,
        &ComponentReceipt {
            schema_version: 1,
            id: delivery.id.clone(),
            version: delivery.version.clone(),
            architecture: ARCHITECTURE.to_string(),
            dependencies: Vec::new(),
            files: delivery.files.iter().map(owned_file).collect(),
        },
    )
    .unwrap();
    invalidate_status(&delivery.id);
    root
}

fn remove_known_install(root: &Path, delivery: &ModelDelivery, extras: &[&str]) {
    let _mutation = super::super::acquire_mutation_guard().unwrap();
    for relative in delivery
        .files
        .iter()
        .map(|file| file.path.as_path())
        .chain(extras.iter().map(Path::new))
    {
        let path = root.join(relative);
        if path.is_file() {
            std::fs::remove_file(path).unwrap();
        }
    }
    let receipt = root.join(RECEIPT_NAME);
    if receipt.is_file() {
        std::fs::remove_file(receipt).unwrap();
    }
    if root.is_dir() {
        std::fs::remove_dir(root).unwrap();
    }
    if let Some(parent) = root.parent()
        && parent.is_dir()
    {
        std::fs::remove_dir(parent).unwrap();
    }
    invalidate_status(&delivery.id);
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(unique_id(label))
}

fn write_zip(path: &Path, entry: &str, bytes: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(entry, zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(bytes).unwrap();
    archive.finish().unwrap();
}

#[test]
fn embedded_delivery_is_complete_immutable_and_exact_sized() {
    let _serial = serial_test();
    catalog().validate().unwrap();
    for kind in [
        ModelKind::QwenSmall,
        ModelKind::QwenLarge,
        ModelKind::StepAudio,
        ModelKind::Magpie,
        ModelKind::Kokoro,
        ModelKind::Supertonic,
        ModelKind::Vieneu,
    ] {
        let model = super::delivery(kind).unwrap();
        assert_eq!(
            model.installed_size_bytes,
            model.files.iter().map(|file| file.size_bytes).sum::<u64>()
        );
        for file in &model.files {
            if let Some(url) = &file.url {
                validate_url(url, None).unwrap();
            }
        }
    }
}

#[test]
fn status_is_size_only_and_acquire_hashes_locked_files_once() {
    let _serial = serial_test();
    let expected = b"pinned-model";
    let model = synthetic_delivery(unique_id("status"), vec![model_file("model.bin", expected)]);
    let root = write_install(&model, &[expected]);
    assert!(validate_status(&model).is_ok());

    std::fs::write(root.join("model.bin"), b"changed-data").unwrap();
    invalidate_status(&model.id);
    assert!(validate_status(&model).is_ok());
    reset_model_hash_passes();
    assert!(acquire_delivery(&model).is_err());
    assert_eq!(model_hash_passes(), 1);
    assert!(validate_status(&model).is_err());

    remove_known_install(&root, &model, &[]);
}

#[test]
fn acquire_rejects_an_unowned_file_after_the_locked_hash_pass() {
    let _serial = serial_test();
    let bytes = b"model";
    let model = synthetic_delivery(unique_id("injected"), vec![model_file("model.bin", bytes)]);
    let root = write_install(&model, &[bytes]);
    assert!(validate_status(&model).is_ok());
    let injected = root.join("injected.py");
    set_post_hash_test_hook({
        let injected = injected.clone();
        move || std::fs::write(injected, b"unowned").unwrap()
    });

    reset_model_hash_passes();
    assert!(acquire_delivery(&model).is_err());
    assert_eq!(model_hash_passes(), 1);
    assert!(injected.is_file());
    remove_known_install(&root, &model, &["injected.py"]);
}

#[test]
fn active_use_locks_files_and_defers_managed_removal() {
    let _serial = serial_test();
    let bytes = b"locked-model";
    let model = synthetic_delivery(unique_id("lease"), vec![model_file("model.bin", bytes)]);
    let root = write_install(&model, &[bytes]);
    let active = acquire_delivery(&model).unwrap();
    assert_eq!(active.root(), root);
    #[cfg(windows)]
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(root.join("model.bin"))
            .is_err()
    );
    assert_eq!(
        super::super::request_remove(&model.id).unwrap(),
        RemovalOutcome::Pending
    );
    assert!(root.exists());
    drop(active);
    assert!(!root.exists());
    invalidate_status(&model.id);
}

#[test]
fn managed_removal_deletes_recorded_content_and_preserves_unknown_content() {
    let _serial = serial_test();
    let exact = b"exact";
    let changed = b"owned";
    let model = synthetic_delivery(
        unique_id("preserve"),
        vec![
            model_file("exact.bin", exact),
            model_file("changed.bin", changed),
        ],
    );
    let root = write_install(&model, &[exact, changed]);
    std::fs::write(root.join("changed.bin"), b"user!").unwrap();
    std::fs::write(root.join("notes.txt"), b"keep").unwrap();

    let outcome = super::super::request_remove(&model.id).unwrap();
    assert!(matches!(outcome, RemovalOutcome::PreservedModified(_)));
    assert!(!root.join("exact.bin").exists());
    assert!(!root.join("changed.bin").exists());
    assert_eq!(std::fs::read(root.join("notes.txt")).unwrap(), b"keep");
    remove_known_install(&root, &model, &["notes.txt"]);
}

#[test]
fn legacy_adoption_copies_only_exact_files_and_preserves_the_source() {
    let _serial = serial_test();
    let exact = b"exact";
    let expected = b"other";
    let model = synthetic_delivery(
        unique_id("adopt"),
        vec![
            model_file("exact.bin", exact),
            model_file("modified.bin", expected),
        ],
    );
    let root = temp_root("adopt-root");
    let legacy = root.join("legacy");
    let stage = root.join("stage");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::create_dir(&stage).unwrap();
    std::fs::write(legacy.join("exact.bin"), exact).unwrap();
    std::fs::write(legacy.join("modified.bin"), b"user!").unwrap();
    std::fs::write(legacy.join("notes.txt"), b"keep").unwrap();

    assert_eq!(
        install::adopt_from_for_test(&model, &legacy, &stage).unwrap(),
        1
    );
    assert_eq!(std::fs::read(stage.join("exact.bin")).unwrap(), exact);
    assert!(!stage.join("modified.bin").exists());
    assert_eq!(std::fs::read(legacy.join("notes.txt")).unwrap(), b"keep");

    for path in [
        stage.join("exact.bin"),
        legacy.join("exact.bin"),
        legacy.join("modified.bin"),
        legacy.join("notes.txt"),
    ] {
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(stage).unwrap();
    std::fs::remove_dir(legacy).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn extraction_rejects_traversal_and_observes_cancellation() {
    let _serial = serial_test();
    let bytes = b"model";
    let model = synthetic_delivery(unique_id("archive"), vec![model_file("model.bin", bytes)]);
    let root = temp_root("archive-root");
    let stage = root.join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    let archive = root.join("pack.zip");
    write_zip(&archive, "../model.bin", bytes);
    assert!(install::extract_for_test(&archive, &stage, &model, &AtomicBool::new(false)).is_err());
    assert!(!root.join("model.bin").exists());
    std::fs::remove_file(&archive).unwrap();

    write_zip(&archive, "model.bin", bytes);
    assert!(install::extract_for_test(&archive, &stage, &model, &AtomicBool::new(true)).is_err());
    assert!(!stage.join("model.bin").exists());
    std::fs::remove_file(archive).unwrap();
    std::fs::remove_dir(stage).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn stale_download_cleanup_matches_only_owned_names() {
    let _serial = serial_test();
    let id = unique_id("scratch");
    let root = state_root().join("component-downloads");
    std::fs::create_dir_all(&root).unwrap();
    let owned = root.join(format!("{id}-123-4.download"));
    assert!(auxiliary::owned_download_name_for_test(
        owned.file_name().unwrap().to_str().unwrap(),
        &id
    ));
    std::fs::write(&owned, b"partial").unwrap();
    let hostile = [
        format!("{id}-user-4.download"),
        format!("{id}-123.download"),
        format!("{id}-123-4.tmp"),
        format!("{id}-123-4.download.extra"),
    ]
    .map(|name| root.join(name));
    for path in &hostile {
        assert!(!auxiliary::owned_download_name_for_test(
            path.file_name().unwrap().to_str().unwrap(),
            &id
        ));
        std::fs::write(path, b"preserve").unwrap();
    }

    let mutation = super::super::acquire_mutation_guard().unwrap();
    auxiliary::cleanup_stale_downloads(&id, &mutation).unwrap();
    assert!(!owned.exists());
    for path in &hostile {
        assert_eq!(std::fs::read(path).unwrap(), b"preserve");
        std::fs::remove_file(path).unwrap();
    }
    let _ = std::fs::remove_dir(root);
}

#[test]
fn cancelled_staging_removal_deletes_only_exact_completed_files() {
    let _serial = serial_test();
    let exact = b"complete";
    let changed = b"expected";
    let model = synthetic_delivery(
        unique_id("staging"),
        vec![
            model_file("exact.bin", exact),
            model_file("changed.bin", changed),
        ],
    );
    let root = auxiliary::staging_root(&model).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("exact.bin"), exact).unwrap();
    std::fs::write(root.join("changed.bin"), b"user-mod").unwrap();
    std::fs::write(root.join("notes.txt"), b"keep").unwrap();

    let (existed, preserved) = auxiliary::remove_staging_for_test(&model).unwrap();
    assert!(existed);
    assert!(!root.join("exact.bin").exists());
    assert_eq!(
        std::fs::read(root.join("changed.bin")).unwrap(),
        b"user-mod"
    );
    assert_eq!(std::fs::read(root.join("notes.txt")).unwrap(), b"keep");
    assert!(!preserved.is_empty());

    std::fs::remove_file(root.join("changed.bin")).unwrap();
    std::fs::remove_file(root.join("notes.txt")).unwrap();
    std::fs::remove_dir(&root).unwrap();
    if let Some(parent) = root.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}
