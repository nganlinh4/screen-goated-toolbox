use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::hash_passes;
use crate::component_registry::receipt::{OwnedComponentFile, file_matches};

fn owned(path: &Path, bytes: &[u8]) -> OwnedComponentFile {
    OwnedComponentFile {
        path: path.file_name().unwrap().into(),
        size_bytes: bytes.len() as u64,
        sha256: format!("{:X}", Sha256::digest(bytes)),
    }
}

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-verified-cache-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, bytes: &[u8]) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

#[test]
fn an_unchanged_verified_file_is_not_hashed_again() {
    let directory = TempRoot::new("unchanged");
    let path = directory.path().join("model.bin");
    write(&path, b"verified model bytes");
    let expected = owned(&path, b"verified model bytes");

    let before = hash_passes();
    assert!(file_matches(&path, &expected).unwrap());
    assert_eq!(hash_passes(), before + 1);
    for _ in 0..5 {
        assert!(file_matches(&path, &expected).unwrap());
    }
    assert_eq!(
        hash_passes(),
        before + 1,
        "status probes reuse the verified state"
    );
}

#[test]
fn a_rewritten_or_replaced_file_is_hashed_again() {
    let directory = TempRoot::new("rewritten");
    let path = directory.path().join("runtime.dll");
    write(&path, b"original bytes");
    let expected = owned(&path, b"original bytes");
    assert!(file_matches(&path, &expected).unwrap());

    // Same size, new content: the write time changes, so it is hashed again.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&path, b"tampered bytes");
    let before = hash_passes();
    assert!(!file_matches(&path, &expected).unwrap());
    assert_eq!(hash_passes(), before + 1);
    assert!(
        !file_matches(&path, &expected).unwrap(),
        "a failed hash is never remembered as verified"
    );
    assert_eq!(hash_passes(), before + 2);

    // Replaced by a different file that carries the same bytes and times.
    write(&path, b"original bytes");
    assert!(file_matches(&path, &expected).unwrap());
    let metadata = std::fs::metadata(&path).unwrap();
    let replacement = directory.path().join("replacement.dll");
    write(&replacement, b"original bytes");
    {
        use std::os::windows::fs::FileTimesExt as _;
        let times = std::fs::FileTimes::new()
            .set_modified(metadata.modified().unwrap())
            .set_created(metadata.created().unwrap());
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&replacement)
            .unwrap();
        file.set_times(times).unwrap();
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert!(file_matches(&path, &expected).unwrap());
    assert_eq!(
        hash_passes(),
        before + 4,
        "a different file at the same path is hashed even with equal times"
    );
}

#[test]
fn a_different_expected_digest_or_size_misses_the_record() {
    let directory = TempRoot::new("digest");
    let path = directory.path().join("voices.bin");
    write(&path, b"voice table");
    let expected = owned(&path, b"voice table");
    assert!(file_matches(&path, &expected).unwrap());

    let other_digest = OwnedComponentFile {
        sha256: format!("{:x}", Sha256::digest(b"another table")),
        ..expected.clone()
    };
    let before = hash_passes();
    assert!(!file_matches(&path, &other_digest).unwrap());
    assert_eq!(hash_passes(), before + 1);

    let other_size = OwnedComponentFile {
        size_bytes: expected.size_bytes + 1,
        ..expected.clone()
    };
    assert!(!file_matches(&path, &other_size).unwrap());
    assert!(file_matches(&path, &expected).unwrap());
    assert_eq!(
        hash_passes(),
        before + 2,
        "a size mismatch drops the record without hashing"
    );
}
