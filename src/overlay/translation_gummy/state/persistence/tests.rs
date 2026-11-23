use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sgt-transcript-persistence-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

fn document() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/translation-gummy/transcript-persistence.json"
    ))
    .unwrap()
}

#[test]
fn gummy_persistence_preserves_old_bytes_on_replacement_failure_and_retries() {
    let contract = document();
    assert_eq!(
        contract["windowsReplacement"]["failedReplacementPreservesOldBytes"],
        true
    );
    assert_eq!(
        contract["windowsReplacement"]["retryAfterFailureSucceeds"],
        true
    );
    assert_eq!(
        contract["windowsReplacement"]["temporaryFilesRemainAfterHandledFailure"],
        false
    );
    let fixture = Fixture::new();
    let path = fixture.0.join("history.json");
    let old = br#"[{"role":"output","text":"previous","lang":""}]"#;
    std::fs::write(&path, old).unwrap();
    assert_eq!(load_at(&path, 200)[0].text, "previous");
    let held = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .open(&path)
        .unwrap();
    let rows: Vec<PersistedTranscript> =
        serde_json::from_value(contract["cases"][0]["persisted"].clone()).unwrap();
    let result = write_snapshot(&path, &rows);
    let bytes_after_failure = std::fs::read(&path).unwrap();
    drop(held);
    assert!(
        result.is_err(),
        "replacement denial must be returned to the save caller"
    );
    assert_eq!(
        bytes_after_failure, old,
        "a failed save must retain the prior complete history"
    );
    assert_eq!(
        std::fs::read_dir(&fixture.0).unwrap().count(),
        1,
        "failed temporary sibling removed"
    );
    write_snapshot(&path, &rows).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        contract["cases"][0]["persisted"]
    );
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
}

#[test]
fn gummy_persistence_round_trips_finalized_rows_and_cleared_history() {
    let fixture = Fixture::new();
    let path = fixture.0.join("history.json");
    let contract = document();
    assert_eq!(contract["version"], 1);
    for case in contract["cases"].as_array().unwrap() {
        let items = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| TranslationGummyTranscriptItem {
                id: item["id"].as_u64().unwrap(),
                role: match item["role"].as_str().unwrap() {
                    "input" => "input",
                    "output" => "output",
                    _ => "separator",
                },
                text: item["text"].as_str().unwrap().into(),
                is_final: item["isFinal"].as_bool().unwrap(),
                lang: item["lang"].as_str().unwrap().into(),
            })
            .collect::<Vec<_>>();
        write_snapshot(&path, &snapshot(&items)).unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, case["persisted"]);
        let loaded = load_at(&path, 200);
        assert!(loaded.iter().all(|item| item.is_final));
        assert_eq!(
            serde_json::to_value(snapshot(&loaded)).unwrap(),
            case["persisted"]
        );
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
    }
}
