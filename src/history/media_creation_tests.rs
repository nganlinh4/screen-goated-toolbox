use super::*;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct Fixture {
    schema_version: u32,
    identity_source: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    pattern: Vec<u8>,
    repeat: usize,
    expected_size: u64,
    expected_sha256: String,
}

struct OwnedDirectory(PathBuf);

impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn shared_media_creation_binds_proof_to_the_retained_creation_handle() {
    let fixture: Fixture = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/history-ui/media-creation.json"
    )))
    .unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.identity_source, "retained_creation_descriptor");
    assert_eq!(fixture.cases.len(), 5);
    let sequence = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = OwnedDirectory(std::env::temp_dir().join(format!(
        "sgt-history-create-{}-{sequence}",
        std::process::id()
    )));
    fs::create_dir(&directory.0).unwrap();
    for case in fixture.cases {
        let payload = case.pattern.repeat(case.repeat);
        let mut media = PendingMedia::create(&directory.0, "source", "bin").unwrap();
        write_proved(&mut media, |writer| writer.write_all(&payload)).unwrap();
        let proof = media.proof.as_ref().unwrap();
        assert_eq!(proof.bytes, case.expected_size, "{}", case.name);
        assert_eq!(proof.sha256, case.expected_sha256, "{}", case.name);
        assert_eq!(
            proof.file_identity,
            file_identity(&media.file).unwrap(),
            "{}",
            case.name
        );
        let path = directory.0.join(&media.filename);
        assert!(
            fs::rename(&path, path.with_extension("moved")).is_err(),
            "creation handle must remain exclusive until publication"
        );
        drop(media);
        assert!(
            !path.exists(),
            "uncommitted media must retire by its handle"
        );
    }
    fs::remove_dir(&directory.0).unwrap();
}

#[test]
fn shared_media_lease_remains_retained_until_persistence_outcome() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/history-ui/media-lease.json"
    )))
    .unwrap();
    assert_eq!(fixture["schema_version"], 1);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let sequence = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = OwnedDirectory(std::env::temp_dir().join(format!(
            "sgt-history-lease-{}-{sequence}",
            std::process::id()
        )));
        fs::create_dir(&directory.0).unwrap();
        let media = write_bytes(&directory.0, "text", "txt", b"source").unwrap();
        let path = directory.0.join(&media.filename);
        let cache = Mutex::new(Vec::new());
        let epoch = AtomicU64::new(u64::from(case["retired"].as_bool().unwrap()));
        let failed = AtomicBool::new(false);
        let context = CommitContext {
            cache: &cache,
            epoch: &epoch,
            max_items: 10,
            media_dir: &directory.0,
            failed: &failed,
        };
        let mut writes = 0;
        commit_saved_with(
            Ok(Some(media)),
            SaveIntent {
                item_type: HistoryType::Text,
                text: "new".into(),
                token: None,
                clear_epoch: 0,
            },
            &context,
            |_| {
                writes += 1;
                assert!(cache.try_lock().unwrap().is_empty());
                assert!(OpenOptions::new().write(true).open(&path).is_err());
                case["write_success"].as_bool().unwrap()
            },
        );
        assert_eq!(writes, case["expected_save_writes"].as_u64().unwrap());
        assert_eq!(
            cache.lock().unwrap().len() as u64,
            case["expected_count"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(path.exists()),
            case["expected_media"].as_u64().unwrap()
        );
        assert_eq!(
            failed.load(Ordering::Acquire),
            case["expected_error"].as_bool().unwrap()
        );
        if path.exists() {
            assert_eq!(fs::read(&path).unwrap(), b"source");
            fs::remove_file(&path).unwrap();
        }
        fs::remove_dir(&directory.0).unwrap();
    }
}
