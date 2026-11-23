use super::*;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;

use windows::Win32::Storage::FileSystem::{DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE};

fn root() -> std::path::PathBuf {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let root = std::env::temp_dir().join(format!(
        "sgt-handle-rename-{:032x}",
        u128::from_le_bytes(nonce)
    ));
    std::fs::create_dir(&root).unwrap();
    root.canonicalize().unwrap()
}

fn file(path: &Path) -> File {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(0)
        .open(path)
        .unwrap();
    file.write_all(b"complete bytes").unwrap();
    file
}

#[test]
fn handle_rename_keeps_exact_names_at_every_utf16_alignment() {
    let root = root();
    for length in 1..=24 {
        let source = root.join("source");
        let target = root.join("x".repeat(length));
        let file = file(&source);
        let name = target.as_os_str().encode_wide().collect::<Vec<_>>();
        rename_file_by_handle(&file, &name, false).unwrap();
        drop(file);
        assert!(!source.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"complete bytes");
        let names = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![target.file_name().unwrap().to_owned()]);
        std::fs::remove_file(target).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn handle_rename_preserves_collision_and_rejects_invalid_names_before_effects() {
    let root = root();
    let source = root.join("source");
    let target = root.join("target");
    std::fs::write(&target, b"existing").unwrap();
    let file = file(&source);
    let name = target.as_os_str().encode_wide().collect::<Vec<_>>();
    assert!(rename_file_by_handle(&file, &name, false).is_err());
    for invalid in [vec![], vec![0], vec![u16::from(b'x'); 32768]] {
        assert!(rename_file_by_handle(&file, &invalid, false).is_err());
    }
    assert_eq!(std::fs::read(&target).unwrap(), b"existing");
    rename_file_by_handle(&file, &name, true).unwrap();
    drop(file);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"complete bytes");
    std::fs::remove_file(target).unwrap();
    std::fs::remove_dir(root).unwrap();
}
