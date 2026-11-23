use super::*;

struct Fixture(Store);
impl Fixture {
    fn new() -> Self {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let name = format!(
            "sgt-journal-fixture-{}-{:x}",
            std::process::id(),
            u64::from_le_bytes(nonce)
        );
        let root = std::env::temp_dir().join(name);
        std::fs::create_dir(&root).unwrap();
        Self(Store(root))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.0.0.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.0
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sgt-journal-fixture-")
        );
        std::fs::remove_dir_all(&self.0.0).unwrap();
    }
}

#[test]
fn finished_recording_survives_a_worker_restart_without_persisting_media_credentials() {
    let fixture = Fixture::new();
    let store = &fixture.0;
    let video = store.0.join("capture.mp4");
    store.begin(7).unwrap();
    store.track(&video).unwrap();
    std::fs::write(&video, b"owned-video").unwrap();
    let result = serde_json::json!({
        "recordingId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "videoFilePath":video,
        "videoUrl":"http://localhost:1/?token=temporary-media-credential",
        "deviceAudioUrl":"temporary-media-credential", "mouseData":[], "inputEvents":[],
    });
    store.ready(&result).unwrap();
    let reopened = Store(store.0.clone());
    let recovered = reopened.read().unwrap().unwrap();
    assert_eq!(recovered.id, 7);
    assert_eq!(
        recovered.result.unwrap()["recordingId"],
        result["recordingId"]
    );
    assert!(
        !std::fs::read_to_string(store.path())
            .unwrap()
            .contains("temporary-media-credential")
    );
    reopened.clear().unwrap();
    assert!(
        video.exists(),
        "project acknowledgement never deletes raw media"
    );
    assert!(reopened.read().unwrap().is_none());
}

#[test]
fn interrupted_capture_keeps_explicit_ownership_of_every_segment_and_sidecar() {
    let fixture = Fixture::new();
    let store = &fixture.0;
    store.begin(8).unwrap();
    for name in [
        "capture.mp4",
        "capture.part000.mp4",
        "capture.part001.mp4",
        "capture_device.wav",
    ] {
        let path = store.0.join(name);
        store.track(&path).unwrap();
        std::fs::write(path, b"unfinished").unwrap();
    }
    let reopened = Store(store.0.clone());
    let journal = reopened.read().unwrap().unwrap();
    assert!(journal.result.is_none());
    assert_eq!(journal.paths.len(), 4);
    assert!(journal.paths.iter().all(|path| path.exists()));
    assert!(
        reopened.begin(9).is_err(),
        "unacknowledged ownership cannot be overwritten"
    );
}

#[test]
fn refuses_existing_or_foreign_media_and_preserves_a_corrupt_journal() {
    let fixture = Fixture::new();
    let store = &fixture.0;
    store.begin(10).unwrap();
    let existing = store.0.join("existing.mp4");
    std::fs::write(&existing, b"user-owned").unwrap();
    assert!(store.track(&existing).is_err());
    assert!(
        store
            .track(&std::env::temp_dir().join("outside.mp4"))
            .is_err()
    );
    assert_eq!(std::fs::read(existing).unwrap(), b"user-owned");
    std::fs::write(store.path(), b"corrupt").unwrap();
    assert!(store.read().is_err());
    assert!(store.begin(11).is_err());
    assert_eq!(std::fs::read(store.path()).unwrap(), b"corrupt");
}

#[test]
fn failed_setup_clears_an_empty_claim_but_keeps_created_files_visible() {
    let fixture = Fixture::new();
    let store = &fixture.0;
    store.begin(12).unwrap();
    let video = store.0.join("setup.mp4");
    store.track(&video).unwrap();
    assert!(store.failed_start("setup failed").unwrap().is_none());
    assert!(store.read().unwrap().is_none());
    store.begin(13).unwrap();
    store.track(&video).unwrap();
    std::fs::write(&video, b"retained").unwrap();
    let failure = store.failed_start("setup failed").unwrap().unwrap();
    assert_eq!(failure.error.as_deref(), Some("setup failed"));
    assert!(video.exists());
}
