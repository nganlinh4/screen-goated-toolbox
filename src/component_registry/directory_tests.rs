use super::*;
use std::io::Read as _;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("sgt-directory-{}-{unique}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    root
}

#[test]
fn retains_root_and_every_parent_until_the_payload_write_finishes() {
    let root = root();
    let (target, guards) = prepare_target(&root, Path::new("one/two/payload")).unwrap();
    assert!(std::fs::rename(root.join("one"), root.join("renamed")).is_err());
    assert!(std::fs::rename(root.join("one/two"), root.join("one/renamed")).is_err());
    std::fs::write(&target, b"payload").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"payload");
    std::fs::remove_file(target).unwrap();
    drop(guards);
    std::fs::rename(root.join("one/two"), root.join("one/renamed")).unwrap();
    std::fs::remove_dir(root.join("one/renamed")).unwrap();
    std::fs::remove_dir(root.join("one")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn preserves_existing_and_creatable_root_policies() {
    let root = root();
    let managed = root.join("managed");
    assert!(prepare_target(&managed, Path::new("a/file")).is_err());
    assert!(!managed.exists());
    let (target, guards) = prepare_target_create(&managed, Path::new("a/file")).unwrap();
    std::fs::write(&target, b"owned").unwrap();
    std::fs::remove_file(target).unwrap();
    drop(guards);
    std::fs::remove_dir(managed.join("a")).unwrap();
    std::fs::remove_dir(managed).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn rejects_invalid_paths_before_creating_any_directory() {
    let root = root();
    let managed = root.join("managed");
    for path in [
        "../outside/file",
        "a/../outside",
        "NUL/file",
        "a./file",
        "a:b/file",
        "",
    ] {
        assert!(
            prepare_target_create(&managed, Path::new(path)).is_err(),
            "{path}"
        );
        assert!(!managed.exists());
    }
    assert!(ensure_directory_tree(&managed, &managed.join("../outside")).is_err());
    assert!(!managed.exists() && !root.join("outside").exists());
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn rejects_regular_files_and_junction_parents_without_writing_through_them() {
    let root = root();
    let managed = root.join("managed");
    let outside = root.join("outside");
    std::fs::create_dir(&managed).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), b"keep").unwrap();
    std::fs::write(managed.join("ordinary"), b"keep").unwrap();
    assert!(prepare_target(&managed, Path::new("ordinary/file")).is_err());
    let link = managed.join("link");
    let executable = crate::system_command::system_executable("cmd.exe").unwrap();
    let mut command = crate::system_command::background_command(executable);
    command
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = crate::process_job::OwnedProcess::spawn(&mut command, ()).unwrap();
    let mut diagnostic = child.take_stderr().unwrap();
    child.resume().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut message = String::new();
            diagnostic.read_to_string(&mut message).unwrap();
            assert!(
                status.success(),
                "junction fixture failed: {status}: {message}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned junction fixture timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.stop_within(Duration::from_secs(1)).unwrap();
    assert!(is_reparse_point(&std::fs::symlink_metadata(&link).unwrap()));
    assert!(prepare_target(&managed, Path::new("link/new/file")).is_err());
    assert!(!outside.join("new").exists());
    assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep");
    std::fs::remove_dir(link).unwrap();
    std::fs::remove_file(managed.join("ordinary")).unwrap();
    std::fs::remove_dir(managed).unwrap();
    std::fs::remove_file(outside.join("sentinel")).unwrap();
    std::fs::remove_dir(outside).unwrap();
    std::fs::remove_dir(root).unwrap();
}
