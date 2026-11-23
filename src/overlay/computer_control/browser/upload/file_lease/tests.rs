use super::*;
use std::fs;

struct Fixture {
    root: PathBuf,
    parent: PathBuf,
    file: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "sgt-upload-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let parent = root.join("parent");
        fs::create_dir(&parent).unwrap();
        let file = parent.join("source.bin");
        fs::write(&file, b"private fixture payload").unwrap();
        Self { root, parent, file }
    }

    fn lease(&self) -> Result<FileLease> {
        FileLease::open(self.file.to_str().unwrap(), &AtomicBool::new(false))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Exact task-owned paths only; no recursive cleanup or unrelated state.
        fs::remove_file(&self.file).unwrap();
        fs::remove_dir(&self.parent).unwrap();
        fs::remove_dir(&self.root).unwrap();
    }
}

#[test]
fn lease_keeps_read_access_and_metadata_without_disclosing_payload() {
    let fixture = Fixture::new();
    let lease = fixture.lease().unwrap();
    assert_eq!(
        fs::read(lease.path().unwrap()).unwrap(),
        b"private fixture payload"
    );
    lease.validate(&AtomicBool::new(false)).unwrap();
    let proposal = lease.proposal().unwrap();
    assert_eq!(proposal["size_bytes"], 23);
    assert!(proposal["file_identity"].as_u64().is_some());
    assert!(!proposal.to_string().contains("private fixture payload"));
}

#[test]
fn lease_blocks_writers_deletion_and_file_or_parent_rename() {
    let fixture = Fixture::new();
    let lease = fixture.lease().unwrap();
    assert!(OpenOptions::new().write(true).open(&fixture.file).is_err());
    assert!(fs::remove_file(&fixture.file).is_err());
    let moved_file = fixture.parent.join("moved.bin");
    let file_rename = fs::rename(&fixture.file, &moved_file);
    if file_rename.is_ok() {
        fs::rename(&moved_file, &fixture.file).unwrap();
    }
    assert!(file_rename.is_err());
    let moved_parent = fixture.root.join("moved-parent");
    let parent_rename = fs::rename(&fixture.parent, &moved_parent);
    if parent_rename.is_ok() {
        fs::rename(&moved_parent, &fixture.parent).unwrap();
    }
    assert!(parent_rename.is_err());
    lease.validate(&AtomicBool::new(false)).unwrap();
    drop(lease);
    fs::write(&fixture.file, b"new bytes").unwrap();
}

#[test]
fn preexisting_writer_prevents_assessing_an_unstable_source() {
    let fixture = Fixture::new();
    let writer = OpenOptions::new().write(true).open(&fixture.file).unwrap();
    assert!(fixture.lease().is_err());
    drop(writer);
    assert!(fixture.lease().is_ok());
}

#[test]
fn cancelled_file_lease_cannot_be_opened_or_revalidated() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(true);
    assert!(FileLease::open(fixture.file.to_str().unwrap(), &cancel).is_err());
    let lease = fixture.lease().unwrap();
    assert!(lease.validate(&cancel).is_err());
}
