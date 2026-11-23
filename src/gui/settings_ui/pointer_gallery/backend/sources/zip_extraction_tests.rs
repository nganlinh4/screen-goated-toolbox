use super::*;
use std::io::Cursor;
use std::sync::{Arc, Barrier};

struct TestDirectory(std::path::PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sgt-cursor-zip-test-{:032x}",
            u128::from_le_bytes(nonce)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn destination(&self) -> std::path::PathBuf {
        self.0.join("pointer.cur")
    }

    fn assert_no_staging(&self) {
        assert!(std::fs::read_dir(&self.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".sgt-cursor-")
        }));
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.destination());
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn zip_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "cursor",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap().into_inner()
}

fn unpack(bytes: Vec<u8>, path: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    extract(&mut archive, 0, path)
}

#[test]
fn accepts_maximum_and_rejects_oversized_or_empty_entries() {
    let dir = TestDirectory::new();
    unpack(
        zip_bytes(&vec![1; MAX_CURSOR_BYTES as usize]),
        &dir.destination(),
    )
    .unwrap();
    assert_eq!(
        std::fs::metadata(dir.destination()).unwrap().len(),
        MAX_CURSOR_BYTES
    );
    std::fs::remove_file(dir.destination()).unwrap();
    assert!(
        unpack(
            zip_bytes(&vec![1; MAX_CURSOR_BYTES as usize + 1]),
            &dir.destination()
        )
        .is_err()
    );
    assert!(unpack(zip_bytes(&[]), &dir.destination()).is_err());
    assert!(!dir.destination().exists());
    dir.assert_no_staging();
}

#[test]
fn forged_smaller_size_and_corrupt_data_leave_no_destination() {
    let dir = TestDirectory::new();
    let mut bytes = zip_bytes(&vec![3; MAX_CURSOR_BYTES as usize + 1]);
    let central = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x01\x02")
        .unwrap();
    bytes[central + 24..central + 28].copy_from_slice(&1_u32.to_le_bytes());
    assert!(unpack(bytes, &dir.destination()).is_err());
    assert!(!dir.destination().exists());
    dir.assert_no_staging();

    let mut bytes = zip_bytes(b"cursor contents");
    let payload = bytes
        .windows(15)
        .position(|window| window == b"cursor contents")
        .unwrap();
    bytes[payload] ^= 1;
    assert!(unpack(bytes, &dir.destination()).is_err());
    assert!(!dir.destination().exists());
    dir.assert_no_staging();
}

#[test]
fn existing_destination_and_competing_extractions_are_never_overwritten() {
    let dir = TestDirectory::new();
    std::fs::write(dir.destination(), b"existing").unwrap();
    assert!(unpack(zip_bytes(b"replacement"), &dir.destination()).is_err());
    assert_eq!(std::fs::read(dir.destination()).unwrap(), b"existing");
    std::fs::remove_file(dir.destination()).unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let workers = [b"first".as_slice(), b"second".as_slice()].map(|content| {
        let barrier = barrier.clone();
        let destination = dir.destination();
        let bytes = zip_bytes(content);
        std::thread::spawn(move || {
            barrier.wait();
            unpack(bytes, &destination)
        })
    });
    let results = workers.map(|worker| worker.join().unwrap());
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "{results:?}"
    );
    let contents = std::fs::read(dir.destination()).unwrap();
    assert!(contents == b"first" || contents == b"second");
    dir.assert_no_staging();
}

#[test]
fn directory_reparse_point_is_rejected() {
    let dir = TestDirectory::new();
    let link = dir.0.join("link");
    if std::os::windows::fs::symlink_dir(&dir.0, &link).is_err() {
        return;
    }
    assert!(unpack(zip_bytes(b"cursor"), &link.join("pointer.cur")).is_err());
    assert!(!dir.destination().exists());
    std::fs::remove_dir(link).unwrap();
    dir.assert_no_staging();
}
