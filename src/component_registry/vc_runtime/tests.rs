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

fn synthetic_file(path: &'static str, bytes: &[u8]) -> VcRuntimeFile {
    VcRuntimeFile {
        path,
        size_bytes: bytes.len() as u64,
        sha256: Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str()),
    }
}

fn synthetic_delivery(files: Vec<VcRuntimeFile>) -> &'static VcRuntimeDelivery {
    let files = Box::leak(files.into_boxed_slice());
    Box::leak(Box::new(VcRuntimeDelivery {
        version: "test-version",
        asset: "test.zip",
        download_url: "https://example.invalid/test.zip",
        size_bytes: 1,
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        unpacked_size_bytes: files.iter().map(|file| file.size_bytes).sum(),
        files,
    }))
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("sgt-vc-{label}-{}-{nonce}", std::process::id()))
}

#[test]
fn delivered_contract_matches_the_complete_support_set() {
    let delivery = delivery().expect("tracked VC delivery is required");
    assert_eq!(
        delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>(),
        [
            "bin/x64/concrt140.dll",
            "bin/x64/msvcp140.dll",
            "bin/x64/msvcp140_1.dll",
            "bin/x64/msvcp140_2.dll",
            "bin/x64/msvcp140_atomic_wait.dll",
            "bin/x64/msvcp140_codecvt_ids.dll",
            "bin/x64/vccorlib140.dll",
            "bin/x64/vcruntime140.dll",
            "bin/x64/vcruntime140_1.dll",
            "bin/x64/vcruntime140_threads.dll",
            "licenses/REDIST.txt",
            "licenses/THIRD-PARTY-NOTICES.txt",
        ]
    );
    assert_eq!(delivery.unpacked_size_bytes, 1_829_454);
}

#[test]
fn verified_delivery_is_present_in_every_build() {
    assert!(delivery().is_ok());
}

#[test]
fn staging_rejects_a_file_in_the_parent_chain() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "sgt-vc-staging-parent-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("bin"), b"not a directory").unwrap();
    assert!(super::staging::prepare_target(&root, Path::new("bin/x64/msvcp140.dll")).is_err());
    std::fs::remove_file(root.join("bin")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn use_lease_locks_verified_files_against_write_and_delete() {
    let root = temp_root("use-lock");
    let bin = root.join("bin/x64");
    std::fs::create_dir_all(&bin).unwrap();
    let bytes = fake_x64_pe();
    let path = bin.join("vcruntime140.dll");
    std::fs::write(&path, &bytes).unwrap();
    let files = [synthetic_file("bin/x64/vcruntime140.dll", &bytes)];

    let locks = lock_component_files(&root, &files).unwrap();
    assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(std::fs::remove_file(&path).is_err());
    drop(locks);

    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(bin).unwrap();
    std::fs::remove_dir(root.join("bin")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn legacy_adoption_requires_exact_x64_files_and_carries_notices() {
    let legacy = temp_root("legacy");
    let staging = temp_root("staging");
    std::fs::create_dir(&legacy).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let runtime = fake_x64_pe();
    let notice = include_bytes!("../../../component-notices/vc14-x64-runtime/REDIST.txt");
    std::fs::write(legacy.join("vcruntime140.dll"), &runtime).unwrap();
    std::fs::write(legacy.join("unknown-user-file.bin"), b"keep").unwrap();
    let delivery = synthetic_delivery(vec![
        synthetic_file("bin/x64/vcruntime140.dll", &runtime),
        synthetic_file("licenses/REDIST.txt", notice),
    ]);

    assert!(super::install::adopt_from_for_test(&legacy, &staging, delivery).unwrap());
    assert_eq!(
        std::fs::read(staging.join("bin/x64/vcruntime140.dll")).unwrap(),
        runtime
    );
    assert_eq!(
        std::fs::read(staging.join("licenses/REDIST.txt")).unwrap(),
        notice
    );
    assert!(legacy.join("unknown-user-file.bin").is_file());

    std::fs::remove_dir_all(legacy).unwrap();
    std::fs::remove_dir_all(staging).unwrap();
}

#[test]
fn legacy_adoption_rejects_wrong_machine_even_with_matching_hash() {
    let legacy = temp_root("wrong-machine");
    let staging = temp_root("wrong-machine-staging");
    std::fs::create_dir(&legacy).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let mut runtime = fake_x64_pe();
    runtime[68..70].copy_from_slice(&0xaa64_u16.to_le_bytes());
    std::fs::write(legacy.join("vcruntime140.dll"), &runtime).unwrap();
    let delivery = synthetic_delivery(vec![synthetic_file("bin/x64/vcruntime140.dll", &runtime)]);

    assert!(super::install::adopt_from_for_test(&legacy, &staging, delivery).is_err());

    std::fs::remove_dir_all(legacy).unwrap();
    std::fs::remove_dir_all(staging).unwrap();
}
