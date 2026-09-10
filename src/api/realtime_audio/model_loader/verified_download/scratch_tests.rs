use super::*;
use std::io::Write as _;
use std::os::windows::process::CommandExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD_ROOT: &str = "SGT_MODEL_SCRATCH_TEST_ROOT";

#[test]
fn abrupt_exit_child() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let destination = Path::new(&root).join("model.bin");
    let mut scratch = DownloadScratch::create(&destination).unwrap();
    scratch.file.write_all(b"unverified partial").unwrap();
    scratch.file.sync_all().unwrap();
    // Exit deliberately bypasses every Rust destructor, as process termination does.
    std::process::exit(71);
}

#[test]
fn process_exit_removes_scratch_and_preserves_destination_and_unowned_files() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "sgt-model-scratch-crash-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("model.bin"), b"previous").unwrap();
    std::fs::write(root.join("model.verified-download"), b"unowned").unwrap();
    let outcome = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(format!(
            "{}::abrupt_exit_child",
            module_path!().split_once("::").unwrap().1
        ))
        .env(CHILD_ROOT, &root)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert_eq!(
        outcome.status.code(),
        Some(71),
        "{}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    assert_eq!(std::fs::read(root.join("model.bin")).unwrap(), b"previous");
    assert_eq!(
        std::fs::read(root.join("model.verified-download")).unwrap(),
        b"unowned"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn publish_replaces_only_destination_and_leaves_no_scratch() {
    let root =
        std::env::temp_dir().join(format!("sgt-model-scratch-publish-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let destination = root.join("model.bin");
    std::fs::write(&destination, b"old").unwrap();
    std::fs::write(root.join("model.unverified-backup"), b"unowned backup").unwrap();
    let mut scratch = DownloadScratch::create(&destination).unwrap();
    scratch.file.write_all(b"verified").unwrap();
    scratch.file.sync_all().unwrap();
    scratch.publish(&destination).unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"verified");
    assert_eq!(
        std::fs::read(root.join("model.unverified-backup")).unwrap(),
        b"unowned backup"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn publish_accepts_every_filename_alignment() {
    let root = std::env::temp_dir().join(format!(
        "sgt-model-scratch-alignment-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    for prefix in ["", "λ", "🦀"] {
        for length in 1..=16 {
            let destination = root.join(format!("{prefix}{}.bin", "x".repeat(length)));
            if length % 2 == 0 {
                std::fs::write(&destination, b"previous verified bytes").unwrap();
            }
            let mut scratch = DownloadScratch::create(&destination).unwrap();
            scratch.file.write_all(b"verified").unwrap();
            scratch
                .publish(&destination)
                .unwrap_or_else(|error| panic!("filename length {length}: {error:#}"));
            assert_eq!(std::fs::read(&destination).unwrap(), b"verified");
        }
    }
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 48);
    std::fs::remove_dir_all(&root).unwrap();
}
