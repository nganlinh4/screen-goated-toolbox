use super::*;
use std::path::PathBuf;

struct TestStore(PathBuf);

impl TestStore {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let root = parent.join(format!(
            "sgt-pending-test-{:016x}",
            u64::from_le_bytes(nonce)
        ));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert!(root.starts_with(parent));
        Self(root)
    }

    fn journal(&self) -> PathBuf {
        self.0.join("journal.json")
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        for name in ["journal.json", "target.json", "link.json"] {
            let _ = std::fs::remove_file(self.0.join(name));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn unknown_ids_survive_supported_records_and_catalog_changes() {
    let store = TestStore::new();
    let journal = store.journal();
    record_at(&journal, "legacy-removed-component").unwrap();
    record_at(&journal, "test-current-component").unwrap();
    let (ready, unresolved) = partition(read_at(&journal).unwrap(), |id| {
        id == "test-current-component"
    });
    assert_eq!(ready, ["test-current-component"]);
    assert_eq!(unresolved, ["legacy-removed-component"]);
    clear_at(&journal, "test-current-component").unwrap();
    let state = read_at(&journal).unwrap();
    assert!(state.component_ids.contains("legacy-removed-component"));
    let (ready_after_catalog_change, unresolved) = partition(state, |_| true);
    assert_eq!(ready_after_catalog_change, ["legacy-removed-component"]);
    assert!(unresolved.is_empty());
}

#[test]
fn malformed_or_unsupported_state_is_never_rewritten() {
    let store = TestStore::new();
    let journal = store.journal();
    for body in [
        b"{invalid".as_slice(),
        br#"{"schemaVersion":2,"componentIds":["legacy-component"]}"#.as_slice(),
        br#"{"schemaVersion":1,"componentIds":["../unsafe"]}"#.as_slice(),
    ] {
        std::fs::write(&journal, body).unwrap();
        assert!(record_at(&journal, "test-current").is_err());
        assert!(clear_at(&journal, "test-current").is_err());
        assert_eq!(std::fs::read(&journal).unwrap(), body);
    }
}

#[test]
fn oversized_state_is_rejected_and_preserved() {
    let store = TestStore::new();
    let body = vec![b' '; MAX_FILE_BYTES as usize + 1];
    std::fs::write(store.journal(), &body).unwrap();
    assert!(record_at(&store.journal(), "test-current").is_err());
    assert_eq!(std::fs::read(store.journal()).unwrap(), body);
}

#[cfg(windows)]
#[test]
fn reparse_journal_is_rejected_without_reading_or_overwriting_target() {
    let store = TestStore::new();
    let target = store.0.join("target.json");
    let link = store.0.join("link.json");
    std::fs::write(&target, b"preserve").unwrap();
    if std::os::windows::fs::symlink_file(&target, &link).is_err() {
        return;
    }
    assert!(record_at(&link, "test-current").is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"preserve");
}
