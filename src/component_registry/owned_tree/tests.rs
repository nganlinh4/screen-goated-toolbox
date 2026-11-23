use std::path::{Path, PathBuf};

use super::{cleanup_owned, regular_files};

fn temp_root(name: &str) -> PathBuf {
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).unwrap();
    let suffix = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let root = std::env::temp_dir().join(format!("sgt-owned-tree-{name}-{suffix}"));
    std::fs::create_dir(&root).unwrap();
    root
}

#[test]
fn regular_files_lists_every_nested_file_relative_to_the_root() {
    let root = temp_root("list");
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    std::fs::write(root.join("bin/x64/tool.exe"), b"tool").unwrap();
    std::fs::write(root.join("receipt.json"), b"{}").unwrap();
    let mut files = regular_files(&root, "test component", 8).unwrap();
    files.sort();
    assert_eq!(
        files,
        [
            PathBuf::from("bin/x64/tool.exe"),
            PathBuf::from("receipt.json")
        ]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn regular_files_bounds_files_directories_and_depth() {
    let root = temp_root("files");
    for index in 0..3 {
        std::fs::write(root.join(format!("file-{index}")), b"x").unwrap();
    }
    let error = regular_files(&root, "test component", 2).unwrap_err();
    assert_eq!(error.to_string(), "test component has too many files");
    std::fs::remove_dir_all(root).unwrap();

    // Empty directories cost nothing to create, so they share the budget.
    let root = temp_root("forest");
    for index in 0..9 {
        std::fs::create_dir(root.join(format!("dir-{index}"))).unwrap();
    }
    let error = regular_files(&root, "test component", 8).unwrap_err();
    assert_eq!(error.to_string(), "test component has too many directories");
    std::fs::remove_dir_all(root).unwrap();

    let root = temp_root("deep");
    let deep = (0..=super::MAX_DEPTH).fold(root.clone(), |path, _| path.join("d"));
    std::fs::create_dir_all(&deep).unwrap();
    let error = regular_files(&root, "test component", 64).unwrap_err();
    assert_eq!(
        error.to_string(),
        "test component directory nesting is too deep"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn regular_files_refuses_a_missing_or_linked_tree() {
    let root = temp_root("links");
    assert!(regular_files(&root.join("missing"), "test component", 8).is_err());
    let outside = temp_root("outside");
    if std::os::windows::fs::symlink_dir(&outside, root.join("linked")).is_ok() {
        let error = regular_files(&root, "test component", 8).unwrap_err();
        assert_eq!(error.to_string(), "test component contains a reparse point");
        std::fs::remove_dir(root.join("linked")).unwrap();
    }
    std::fs::remove_dir(outside).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn cleanup_removes_only_declared_files_and_preserves_unknowns() {
    let root = temp_root("cleanup");
    std::fs::create_dir_all(root.join("bin/x64")).unwrap();
    std::fs::write(root.join("bin/x64/owned.exe"), b"owned").unwrap();
    std::fs::write(root.join("unknown.bin"), b"unknown").unwrap();
    cleanup_owned(&root, &["bin/x64/owned.exe".into()]).unwrap();
    assert!(!root.join("bin").exists());
    assert_eq!(std::fs::read(root.join("unknown.bin")).unwrap(), b"unknown");
    std::fs::remove_file(root.join("unknown.bin")).unwrap();
    std::fs::remove_dir(&root).unwrap();
    assert!(cleanup_owned(&root, &["owned.bin".into()]).is_ok());
}

#[test]
fn cleanup_never_follows_an_owned_file_symlink() {
    let root = temp_root("reparse");
    let outside = root.with_extension("outside");
    std::fs::write(&outside, b"preserve").unwrap();
    let link = root.join("owned.bin");
    if std::os::windows::fs::symlink_file(&outside, &link).is_ok() {
        assert!(cleanup_owned(&root, &[Path::new("owned.bin").to_path_buf()]).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"preserve");
        std::fs::remove_file(link).unwrap();
    }
    std::fs::remove_file(outside).unwrap();
    std::fs::remove_dir(root).unwrap();
}
