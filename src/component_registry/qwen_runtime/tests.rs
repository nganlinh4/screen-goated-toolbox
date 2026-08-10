use super::*;
use sha2::{Digest, Sha256};

fn fake_x64_pe() -> Vec<u8> {
    let mut bytes = vec![0_u8; 72];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64_u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&0x8664_u16.to_le_bytes());
    bytes
}

fn synthetic_delivery(files: Vec<QwenRuntimeFile>) -> &'static QwenRuntimeDelivery {
    let files = Box::leak(files.into_boxed_slice());
    Box::leak(Box::new(QwenRuntimeDelivery {
        version: "test-version",
        archives: &[QwenRuntimeArchive {
            url: "https://example.invalid/runtime.zip",
            size_bytes: 1,
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        }],
        unpacked_size_bytes: files.iter().map(|file| file.size_bytes).sum(),
        files,
    }))
}

fn synthetic_file(path: &'static str, bytes: &[u8]) -> QwenRuntimeFile {
    let hash = Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str());
    QwenRuntimeFile {
        archive_index: 0,
        archive_path: path,
        path,
        size_bytes: bytes.len() as u64,
        sha256: hash,
    }
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("sgt-qwen-{label}-{}-{nonce}", std::process::id()))
}

#[test]
fn legacy_adoption_requires_every_exact_file_and_preserves_unknown_files() {
    let legacy = temp_root("legacy");
    let staging = temp_root("staging");
    std::fs::create_dir(&legacy).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let runtime = fake_x64_pe();
    let marker = b"pinned-build\n";
    std::fs::write(legacy.join("runtime.dll"), &runtime).unwrap();
    std::fs::write(legacy.join("build-version"), marker).unwrap();
    std::fs::write(legacy.join("unknown-user-file.bin"), b"keep").unwrap();
    let delivery = synthetic_delivery(vec![
        synthetic_file("bin/x64/runtime.dll", &runtime),
        synthetic_file("metadata/build-version", marker),
    ]);

    assert!(super::install::adopt_from_for_test(&legacy, &staging, delivery).unwrap());
    assert_eq!(
        std::fs::read(staging.join("bin/x64/runtime.dll")).unwrap(),
        runtime
    );
    assert_eq!(
        std::fs::read(staging.join("metadata/build-version")).unwrap(),
        marker
    );
    assert!(legacy.join("unknown-user-file.bin").is_file());

    std::fs::remove_dir_all(legacy).unwrap();
    std::fs::remove_dir_all(staging).unwrap();
}

#[test]
fn legacy_adoption_rejects_modified_native_file() {
    let legacy = temp_root("modified-legacy");
    let staging = temp_root("modified-staging");
    std::fs::create_dir(&legacy).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let expected = fake_x64_pe();
    let mut modified = expected.clone();
    modified[71] = 1;
    std::fs::write(legacy.join("runtime.dll"), modified).unwrap();
    let delivery = synthetic_delivery(vec![synthetic_file("bin/x64/runtime.dll", &expected)]);

    assert!(!super::install::adopt_from_for_test(&legacy, &staging, delivery).unwrap());
    assert!(std::fs::read_dir(&staging).unwrap().next().is_none());

    std::fs::remove_dir_all(legacy).unwrap();
    std::fs::remove_dir_all(staging).unwrap();
}

#[test]
fn qwen_catalog_dependency_blocks_vc_removal_while_receipt_remains() {
    let component = super::super::embedded_catalog()
        .components
        .iter()
        .find(|component| component.id == COMPONENT_ID)
        .expect("Qwen3 runtime catalog entry");
    assert_eq!(component.dependencies, [VC_COMPONENT_ID]);
}

#[test]
fn tracked_qwen_delivery_is_present_in_every_build() {
    assert!(delivery().is_ok());
}

#[test]
fn active_qwen_lease_makes_removal_pending() {
    let lease = super::super::acquire(COMPONENT_ID).unwrap();
    assert_eq!(
        super::super::request_remove(COMPONENT_ID).unwrap(),
        RemovalOutcome::Pending
    );
    drop(lease);
}

#[test]
fn locked_runtime_file_rejects_tamper_until_load_guard_drops() {
    let root = temp_root("locked-file");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("runtime.dll");
    std::fs::write(&path, fake_x64_pe()).unwrap();

    let locked = open_locked_regular_file(&path).unwrap();
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .is_err()
    );
    drop(locked);
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();

    std::fs::remove_dir_all(root).unwrap();
}
