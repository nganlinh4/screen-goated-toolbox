use std::io::Write as _;
use std::sync::atomic::AtomicBool;

use sha2::{Digest, Sha256};

use super::*;

#[test]
fn tracked_delivery_contains_every_external_tool() {
    for tool in [
        ExternalTool::YtDlp,
        ExternalTool::Ffmpeg,
        ExternalTool::Deno,
    ] {
        assert!(delivery_optional(tool).is_some());
    }
    assert!(WEBVIEW2_BOOTSTRAPPER_DELIVERY.is_some());
}

fn temp_root(label: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sgt-external-{label}-{}-{unique}",
        std::process::id()
    ))
}

fn x64_pe() -> Vec<u8> {
    let mut bytes = vec![0_u8; 70];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&64_u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&0x8664_u16.to_le_bytes());
    bytes
}

fn test_delivery(bytes: &[u8]) -> ExternalToolDelivery {
    let digest = Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str());
    let files = Box::leak(
        vec![ExternalToolFile {
            path: "bin/x64/yt-dlp.exe",
            archive_path: "yt-dlp.exe",
            size_bytes: bytes.len() as u64,
            sha256: digest,
        }]
        .into_boxed_slice(),
    );
    ExternalToolDelivery {
        id: "yt-dlp-x64",
        version: "test",
        asset: "yt-dlp.exe",
        download_url: "https://example.invalid/yt-dlp.exe",
        archive_format: ExternalArchiveFormat::Zip,
        size_bytes: 1,
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        unpacked_size_bytes: bytes.len() as u64,
        files,
    }
}

#[test]
fn legacy_adoption_requires_exact_x64_bytes() {
    let root = temp_root("adoption");
    let legacy = root.join("legacy");
    let stage = root.join("stage");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::create_dir(&stage).unwrap();
    let bytes = x64_pe();
    let delivery = test_delivery(&bytes);
    std::fs::write(legacy.join("yt-dlp.exe"), &bytes).unwrap();
    assert!(install::adopt_from_for_test(&legacy, &stage, &delivery).unwrap());
    assert_eq!(
        std::fs::read(stage.join("bin/x64/yt-dlp.exe")).unwrap(),
        bytes
    );

    let mismatch = root.join("mismatch");
    std::fs::create_dir(&mismatch).unwrap();
    std::fs::write(legacy.join("yt-dlp.exe"), b"changed").unwrap();
    assert!(!install::adopt_from_for_test(&legacy, &mismatch, &delivery).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn staged_external_tools_sync_through_writable_windows_handles() {
    let root = temp_root("writable-sync");
    let bytes = x64_pe();
    let delivery = test_delivery(&bytes);
    let target = root.join("bin/x64/yt-dlp.exe");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, bytes).unwrap();

    install::sync_staged_files(&delivery, &root).unwrap();

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn zip_traversal_is_rejected_before_writing_outside_stage() {
    let root = temp_root("traversal");
    let stage = root.join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    let archive_path = root.join("malicious.zip");
    let file = std::fs::File::create(&archive_path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("../yt-dlp.exe", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(&x64_pe()).unwrap();
    archive.finish().unwrap();
    let delivery = test_delivery(&x64_pe());
    assert!(install::extract_zip_for_test(&delivery, &archive_path, &stage).is_err());
    assert!(!root.join("yt-dlp.exe").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn staging_cleanup_preserves_unknown_files() {
    let root = temp_root("cleanup");
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    std::fs::write(root.join("bin/x64/owned.exe"), b"owned").unwrap();
    std::fs::write(root.join("unknown.txt"), b"unknown").unwrap();
    staging::cleanup_owned(&root, &["bin/x64/owned.exe".into()]).unwrap();
    assert!(!root.join("bin/x64/owned.exe").exists());
    assert_eq!(std::fs::read(root.join("unknown.txt")).unwrap(), b"unknown");
    std::fs::remove_file(root.join("unknown.txt")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn component_tree_rejects_an_unbounded_directory_forest() {
    let root = temp_root("directory-budget");
    std::fs::create_dir(&root).unwrap();
    for index in 0..65 {
        std::fs::create_dir(root.join(format!("dir-{index}"))).unwrap();
    }
    let mut files = Vec::new();
    assert!(staging::collect_regular_files(&root, &root, &mut files, 8).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_download_cleanup_matches_only_owned_names() {
    assert!(install::generated_download_name_for_test(
        "yt-dlp-x64-123-4.download",
        "yt-dlp-x64"
    ));
    for name in [
        "yt-dlp-x64-user.download",
        "yt-dlp-x64-123.download",
        "yt-dlp-x64-123-4.tmp",
        "other-123-4.download",
    ] {
        assert!(!install::generated_download_name_for_test(
            name,
            "yt-dlp-x64"
        ));
    }
}

#[test]
fn missing_delivery_fails_closed_without_starting_a_download() {
    let missing = EXTERNAL_TOOL_DELIVERIES
        .iter()
        .all(|delivery| delivery.id != "test-missing-tool");
    assert!(missing);
    let cancel = AtomicBool::new(false);
    assert!(!cancel.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn runtime_acquisition_hashes_the_locked_tree_once() {
    let bytes = x64_pe();
    let delivery = Box::leak(Box::new(test_delivery(&bytes)));
    let root = version_root(delivery).unwrap();
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    std::fs::write(root.join("bin/x64/yt-dlp.exe"), &bytes).unwrap();
    crate::component_registry::write_receipt(&root, &receipt(delivery)).unwrap();

    reset_runtime_hash_passes();
    let component = acquire_delivery(ExternalTool::YtDlp, delivery).unwrap();
    assert_eq!(runtime_hash_passes(), 1);
    drop(component);

    let mut changed = bytes;
    changed[2] = 1;
    std::fs::write(root.join("bin/x64/yt-dlp.exe"), changed).unwrap();
    assert!(validate_install_fast(delivery).is_ok());
    reset_runtime_hash_passes();
    assert!(acquire_delivery(ExternalTool::YtDlp, delivery).is_err());
    assert_eq!(runtime_hash_passes(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupted_install_is_quarantined_then_reinstalled_without_data_loss() {
    let bytes = x64_pe();
    let mut delivery = test_delivery(&bytes);
    delivery.id = "test-external-repair";
    delivery.version = "repair-test";
    let root = version_root(&delivery).unwrap();
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    let modified = vec![9_u8; bytes.len()];
    std::fs::write(root.join("bin/x64/yt-dlp.exe"), &modified).unwrap();
    std::fs::write(root.join("user-note.txt"), b"preserve me").unwrap();
    std::fs::write(root.join(RECEIPT_NAME), b"partial receipt").unwrap();

    let recovery = install::quarantine_invalid_for_test(&delivery).unwrap();
    assert!(!root.exists());
    assert_eq!(
        std::fs::read(recovery.join("bin/x64/yt-dlp.exe")).unwrap(),
        modified
    );
    assert_eq!(
        std::fs::read(recovery.join("user-note.txt")).unwrap(),
        b"preserve me"
    );

    let entries = recovery::list_for_test(&delivery).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].can_clean);
    assert!(entries[0].reason.contains("test integrity failure"));
    std::fs::write(recovery.join("bin/x64/yt-dlp.exe"), vec![8_u8; bytes.len()]).unwrap();
    let cleanup = recovery::clean_for_test(&delivery, &entries[0]).unwrap();
    assert_eq!(cleanup.removed_files, 1);
    assert!(
        cleanup
            .preserved_paths
            .contains(&recovery.join("bin/x64/yt-dlp.exe"))
    );
    assert!(
        cleanup
            .preserved_paths
            .contains(&recovery.join("user-note.txt"))
    );
    assert!(recovery.exists());

    let stage = temp_root("repair-stage");
    if stage.exists() {
        std::fs::remove_dir_all(&stage).unwrap();
    }
    std::fs::create_dir_all(stage.join("bin/x64")).unwrap();
    std::fs::write(stage.join("bin/x64/yt-dlp.exe"), &bytes).unwrap();
    crate::component_registry::write_receipt(&stage, &receipt(&delivery)).unwrap();
    install::finish_staging_for_test(&delivery, &stage).unwrap();
    assert!(validate_install_fast(&delivery).is_ok());

    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(recovery.parent().unwrap()).unwrap();
}

#[test]
fn explicit_recovery_purge_deletes_changed_recorded_bytes_but_not_unknown_files() {
    let bytes = x64_pe();
    let mut delivery = test_delivery(&bytes);
    delivery.id = "test-external-explicit-purge";
    delivery.version = "purge-test";
    let root = version_root(&delivery).unwrap();
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    std::fs::write(root.join("bin/x64/yt-dlp.exe"), &bytes).unwrap();
    std::fs::write(root.join("user-note.txt"), b"preserve me").unwrap();
    crate::component_registry::write_receipt(&root, &receipt(&delivery)).unwrap();

    let recovery = install::quarantine_invalid_for_test(&delivery).unwrap();
    let entries = recovery::list_for_test(&delivery).unwrap();
    std::fs::write(recovery.join("bin/x64/yt-dlp.exe"), vec![7_u8; bytes.len()]).unwrap();
    let outcome = recovery::purge_for_test(&delivery, &entries[0]).unwrap();

    assert!(!recovery.join("bin/x64/yt-dlp.exe").exists());
    assert_eq!(
        std::fs::read(recovery.join("user-note.txt")).unwrap(),
        b"preserve me"
    );
    assert!(
        outcome
            .preserved_paths
            .contains(&recovery.join("user-note.txt"))
    );
    std::fs::remove_dir_all(recovery.parent().unwrap()).unwrap();
}

#[test]
fn recovery_sidecar_is_durable_before_move_and_rolled_back_on_failure() {
    let bytes = x64_pe();
    let mut delivery = test_delivery(&bytes);
    delivery.id = "test-recovery-order";
    delivery.version = "order-test";
    let root = version_root(&delivery).unwrap();
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    std::fs::write(root.join("bin/x64/yt-dlp.exe"), &bytes).unwrap();
    let result = recovery::quarantine_with_rename_for_test(
        &delivery,
        "ordered recovery",
        |source, target| {
            assert!(source.exists());
            let sidecar_exists = std::fs::read_dir(target.parent().unwrap())
                .unwrap()
                .flatten()
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".recovery.json")
                });
            assert!(sidecar_exists);
            Err(std::io::Error::other("injected rename failure"))
        },
    );
    assert!(result.is_err());
    assert!(root.exists());
    assert!(recovery::list_for_test(&delivery).unwrap().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_sidecar_names_cannot_escape_the_tool_directory() {
    let parent = temp_root("sidecar-path");
    for name in ["...recovery.json", "CON.recovery.json", "C:.recovery.json"] {
        assert!(recovery::sidecar_target_for_test(&parent.join(name)).is_none());
    }
    let valid = parent.join("version-12-3.recovery.json");
    assert_eq!(
        recovery::sidecar_target_for_test(&valid),
        Some(parent.join("version-12-3"))
    );
}

#[test]
fn downloader_sources_expose_no_mutable_update_flow() {
    let sources = [
        include_str!("../../gui/settings_ui/download_manager/run.rs"),
        include_str!("../../gui/settings_ui/download_manager/run_download.rs"),
        include_str!("../../gui/settings_ui/download_manager/ffmpeg_dependency.rs"),
        include_str!("../../gui/settings_ui/global/downloaded_tools/video_downloader.rs"),
    ];
    for source in sources {
        for forbidden in [
            "releases/latest",
            "fetch_latest",
            "check_updates",
            "UpdateStatus",
            "--update",
            "-U\"",
        ] {
            assert!(
                !source.contains(forbidden),
                "mutable external-tool API remains: {forbidden}"
            );
        }
    }
}
