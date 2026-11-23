use super::*;
use std::io::Cursor;

fn image(offset: u32) -> Vec<u8> {
    let mut bytes = vec![0; offset as usize + 6];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&offset.to_le_bytes());
    bytes[offset as usize..offset as usize + 4].copy_from_slice(b"PE\0\0");
    bytes[offset as usize + 4..].copy_from_slice(&0x8664_u16.to_le_bytes());
    bytes
}

fn inspect(bytes: Vec<u8>) -> Result<()> {
    let length = bytes.len() as u64;
    inspect_header(&mut Cursor::new(bytes), length)
}

#[test]
fn accepts_x64_headers_at_both_offset_bounds() {
    for offset in [64, 128, MAX_PE_OFFSET as u32] {
        inspect(image(offset)).unwrap();
    }
}

#[test]
fn rejects_incomplete_headers_and_invalid_offsets() {
    let bytes = image(64);
    for end in 0..bytes.len() {
        assert!(inspect(bytes[..end].to_vec()).is_err(), "length {end}");
    }
    for offset in [0, 1, 63, 65, MAX_PE_OFFSET as u32 + 1, u32::MAX] {
        let mut bytes = image(64);
        bytes[60..64].copy_from_slice(&offset.to_le_bytes());
        assert!(inspect(bytes).is_err(), "offset {offset}");
    }
    assert!(inspect(image(MAX_PE_OFFSET as u32 + 1)).is_err());
}

#[test]
fn rejects_wrong_signatures_and_other_architectures() {
    for index in [0, 1, 64, 65, 66, 67] {
        let mut bytes = image(64);
        bytes[index] ^= 0xff;
        assert!(inspect(bytes).is_err(), "signature byte {index}");
    }
    for architecture in [0, 0x14c, 0xaa64, 0xffff] {
        let mut bytes = image(64);
        bytes[68..70].copy_from_slice(&(architecture as u16).to_le_bytes());
        assert!(inspect(bytes).is_err(), "architecture {architecture}");
    }
}

#[test]
#[cfg(target_arch = "x86_64")]
fn validates_the_current_binary_without_executing_another_process() {
    validate_x64_pe(&std::env::current_exe().unwrap()).unwrap();
}

#[test]
fn inspection_lock_blocks_mutation_and_rejects_directories() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("sgt-native-file-{}-{unique}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("native.bin");
    std::fs::write(&path, image(64)).unwrap();
    validate_x64_pe(&path).unwrap();
    let file = open_regular(&path).unwrap();
    assert!(std::fs::write(&path, b"changed").is_err());
    assert!(std::fs::remove_file(&path).is_err());
    assert!(std::fs::rename(&path, root.join("renamed.bin")).is_err());
    assert!(open_regular(&root).is_err());
    assert!(validate_x64_pe(&root.join("missing")).is_err());
    drop(file);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn a_file_symlink_is_refused_rather_than_followed() {
    let root = std::env::temp_dir().join(format!(
        "sgt-native-file-link-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir(&root).unwrap();
    let target = root.join("target.bin");
    let link = root.join("link.bin");
    std::fs::write(&target, image(64)).unwrap();
    // Creating a symlink needs Developer Mode or elevation on Windows.
    if std::os::windows::fs::symlink_file(&target, &link).is_ok() {
        assert!(open_regular(&link).is_err());
        assert!(validate_x64_pe(&link).is_err());
    }
    assert!(open_regular(&target).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lock_verified_pins_matching_bytes_and_rejects_size_or_hash_drift() {
    use sha2::Digest as _;
    let root = std::env::temp_dir().join(format!(
        "sgt-lock-verified-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let bytes = vec![7_u8; 300 * 1024];
    std::fs::write(root.join("bin/tool.dll"), &bytes).unwrap();
    let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    let size = bytes.len() as u64;

    let locked = lock_verified(&root, "bin/tool.dll", size, &digest, "changed").unwrap();
    assert!(std::fs::write(root.join("bin/tool.dll"), b"tampered").is_err());
    drop(locked);

    let error = lock_verified(&root, "bin/tool.dll", size + 1, &digest, "size drift").unwrap_err();
    assert_eq!(error.to_string(), "size drift");
    let wrong = format!("{:x}", sha2::Sha256::digest(b"other"));
    let error = lock_verified(&root, "bin/tool.dll", size, &wrong, "hash drift").unwrap_err();
    assert_eq!(error.to_string(), "hash drift");
    assert!(lock_verified(&root, "../outside.dll", size, &digest, "escape").is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[cfg(not(feature = "recorder-worker"))]
#[test]
fn canonical_launch_paths_require_the_expected_regular_kind() {
    let root = std::env::temp_dir().join(format!(
        "sgt-canonical-launch-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let file = root.join("bin/worker.exe");
    std::fs::write(&file, image(64)).unwrap();

    let canonical =
        canonical_regular_file(&root.join("bin/../bin/worker.exe"), "test worker").unwrap();
    assert_eq!(canonical, std::fs::canonicalize(&file).unwrap());
    assert!(canonical_regular_dir(&root.join("bin"), "test runtime").is_ok());

    let error = canonical_regular_file(&root.join("bin"), "test worker").unwrap_err();
    assert_eq!(error.to_string(), "test worker path is unsafe");
    let error = canonical_regular_dir(&file, "test runtime").unwrap_err();
    assert_eq!(error.to_string(), "test runtime directory is unsafe");
    assert!(canonical_regular_file(&root.join("missing.exe"), "test worker").is_err());

    // Creating a link needs Developer Mode or elevation on Windows.
    let linked = root.join("linked.exe");
    if std::os::windows::fs::symlink_file(&file, &linked).is_ok() {
        // canonicalize resolves the link, so the final target is what is checked.
        assert_eq!(
            canonical_regular_file(&linked, "test worker").unwrap(),
            std::fs::canonicalize(&file).unwrap()
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn plain_files_and_directories_are_not_reparse_points() {
    let dir = std::env::temp_dir().join(format!(
        "sgt-reparse-plain-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("plain.txt");
    std::fs::write(&file, b"x").unwrap();
    assert!(!is_reparse_point(
        &std::fs::symlink_metadata(&file).unwrap()
    ));
    assert!(!is_reparse_point(&std::fs::symlink_metadata(&dir).unwrap()));
    let link = dir.join("link.txt");
    if std::os::windows::fs::symlink_file(&file, &link).is_ok() {
        assert!(is_reparse_point(&std::fs::symlink_metadata(&link).unwrap()));
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
