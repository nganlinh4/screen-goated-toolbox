use super::*;
use serde_json::{Value, json};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path =
            std::env::temp_dir().join(format!("sgt-telemetry-{:032x}", u128::from_le_bytes(nonce)));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("events.jsonl")
    }
    fn records(&self) -> Vec<Value> {
        let bytes = std::fs::read(self.path()).unwrap();
        assert_eq!(bytes.last(), Some(&b'\n'));
        bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.path());
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn repeated_compaction_keeps_recent_complete_unicode_records_and_bounds_bytes() {
    let directory = Directory::new();
    let mut sink = Sink::open(&directory.path()).unwrap();
    for index in 0..100 {
        let line = json!({"index": index, "text": "😀 가나다"}).to_string();
        sink.append(line.as_bytes(), Some((1024, 512))).unwrap();
        assert!(sink.file.metadata().unwrap().len() <= 1024);
    }
    let records = directory.records();
    assert_eq!(records[0]["event"], "telemetry_compacted");
    assert_eq!(records.last().unwrap()["index"], 99);
    let indices = records
        .iter()
        .filter_map(|record| record["index"].as_u64())
        .collect::<Vec<_>>();
    assert!(indices.windows(2).all(|pair| pair[1] == pair[0] + 1));
}

#[test]
fn oversized_record_does_not_change_existing_log() {
    let directory = Directory::new();
    let mut sink = Sink::open(&directory.path()).unwrap();
    sink.append(b"{\"keep\":true}", Some((1024, 512))).unwrap();
    let before = std::fs::read(directory.path()).unwrap();
    assert!(sink.append(&[b'x'; 1024], Some((1024, 512))).is_err());
    assert_eq!(std::fs::read(directory.path()).unwrap(), before);
}

#[test]
fn hard_linked_file_is_not_compacted_or_appended() {
    let directory = Directory::new();
    let sentinel = directory.0.join("sentinel");
    std::fs::write(&sentinel, b"preserve").unwrap();
    std::fs::hard_link(&sentinel, directory.path()).unwrap();
    let mut sink = Sink::open(&directory.path()).unwrap();
    assert!(sink.append(b"{}", Some((1024, 512))).is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"preserve");
    drop(sink);
    std::fs::remove_file(sentinel).unwrap();
}

#[test]
fn interrupted_append_is_removed_before_next_complete_record() {
    let directory = Directory::new();
    std::fs::write(directory.path(), b"{\"index\":0}\n{\"partial\":").unwrap();
    let mut sink = Sink::open(&directory.path()).unwrap();
    sink.append(b"{\"index\":1}", None).unwrap();
    assert_eq!(
        directory.records(),
        [json!({"index":0}), json!({"index":1})]
    );
}

#[test]
fn oversized_interrupted_session_record_preserves_existing_bytes() {
    let directory = Directory::new();
    let mut bytes = b"{\"keep\":true}\n".to_vec();
    bytes.extend(vec![b'x'; RETAIN_BYTES as usize + 1]);
    std::fs::write(directory.path(), &bytes).unwrap();
    let mut sink = Sink::open(&directory.path()).unwrap();
    assert!(sink.append(b"{}", None).is_err());
    assert_eq!(std::fs::read(directory.path()).unwrap(), bytes);
}

#[test]
fn oversized_incomplete_global_log_is_recovered_within_limit() {
    let directory = Directory::new();
    std::fs::write(directory.path(), vec![b'x'; 4096]).unwrap();
    let mut sink = Sink::open(&directory.path()).unwrap();
    sink.append(b"{\"new\":true}", Some((1024, 512))).unwrap();
    assert!(sink.file.metadata().unwrap().len() <= 1024);
    assert_eq!(directory.records().last().unwrap()["new"], true);
}

#[test]
fn separate_handles_serialize_compaction_and_append() {
    let directory = Directory::new();
    std::thread::scope(|scope| {
        for producer in 0..4 {
            let path = directory.path();
            scope.spawn(move || {
                let mut sink = Sink::open(&path).unwrap();
                for index in 0..100 {
                    sink.append(
                        json!({"producer":producer,"index":index})
                            .to_string()
                            .as_bytes(),
                        Some((2048, 1024)),
                    )
                    .unwrap();
                }
            });
        }
    });
    assert!(std::fs::metadata(directory.path()).unwrap().len() <= 2048);
    assert!(
        directory
            .records()
            .iter()
            .any(|record| record["index"] == 99)
    );
}

#[test]
fn switching_session_releases_previous_handle_and_reset_can_remove_logs() {
    let directory = Directory::new();
    let first = directory.0.join("first.jsonl");
    let mut writers = Writers::default();
    writers.append(&first, b"{}", false).unwrap();
    writers.append(&directory.path(), b"{}", false).unwrap();
    std::fs::remove_file(first).unwrap();
    drop(writers);
    std::fs::remove_file(directory.path()).unwrap();
    std::fs::remove_dir(&directory.0).unwrap();
}

#[test]
fn existing_oversized_log_is_compacted_on_first_append() {
    let directory = Directory::new();
    std::fs::write(directory.path(), b"{\"old\":true}\n".repeat(1000)).unwrap();
    let mut sink = Sink::open(&directory.path()).unwrap();
    sink.append(b"{\"new\":true}", Some((1024, 512))).unwrap();
    assert!(sink.file.metadata().unwrap().len() <= 1024);
    assert_eq!(directory.records().last().unwrap()["new"], true);
}

#[test]
fn child_writer() {
    let Some(path) = std::env::var_os("SGT_TELEMETRY_WRITER_TEST_PATH") else {
        return;
    };
    let mut sink = Sink::open(Path::new(&path)).unwrap();
    for index in 0..50 {
        sink.append(
            json!({"producer":"child","index":index})
                .to_string()
                .as_bytes(),
            None,
        )
        .unwrap();
    }
}

#[test]
fn separate_processes_append_without_torn_or_missing_records() {
    use std::process::{Command, Stdio};
    let directory = Directory::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "overlay::computer_control::telemetry::writer::tests::child_writer",
        ])
        .env("SGT_TELEMETRY_WRITER_TEST_PATH", directory.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut sink = Sink::open(&directory.path()).unwrap();
    for index in 0..50 {
        sink.append(
            json!({"producer":"parent","index":index})
                .to_string()
                .as_bytes(),
            None,
        )
        .unwrap();
    }
    assert!(child.wait().unwrap().success());
    let records = directory.records();
    assert_eq!(records.len(), 100);
    for producer in ["parent", "child"] {
        let indices = records
            .iter()
            .filter(|record| record["producer"] == producer)
            .map(|record| record["index"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(indices, (0..50).collect::<Vec<_>>());
    }
}

#[test]
fn capped_session_trace_keeps_its_beginning_and_marks_the_cap_once() {
    let directory = Directory::new();
    let mut sink = Sink::open(&directory.path()).unwrap();
    for index in 0..50 {
        let line = json!({"index": index, "text": "가나다"}).to_string();
        sink.append_capped(line.as_bytes(), 512).unwrap();
        assert!(sink.file.metadata().unwrap().len() <= 512);
    }
    drop(sink);
    let mut reopened = Sink::open(&directory.path()).unwrap();
    reopened.append_capped(b"{\"late\":true}", 512).unwrap();
    let records = directory.records();
    assert_eq!(records[0]["index"], 0);
    let indices = records
        .iter()
        .filter_map(|record| record["index"].as_u64())
        .collect::<Vec<_>>();
    assert!(indices.windows(2).all(|pair| pair[1] == pair[0] + 1));
    assert_eq!(records.last().unwrap()["event"], "telemetry_session_capped");
    assert_eq!(
        records
            .iter()
            .filter(|record| record["event"] == "telemetry_session_capped")
            .count(),
        1
    );
}

#[test]
fn session_writer_stops_growing_at_the_session_cap() {
    let directory = Directory::new();
    let filler = SESSION_MAX_BYTES as usize - SESSION_CAP_MARKER.len() - 3;
    std::fs::write(directory.path(), b"{}\n".repeat(filler / 3)).unwrap();
    let before = std::fs::metadata(directory.path()).unwrap().len();
    let mut writers = Writers::default();
    for _ in 0..3 {
        writers
            .append(&directory.path(), &[b' '; 64], false)
            .unwrap();
    }
    drop(writers);
    let bytes = std::fs::read(directory.path()).unwrap();
    assert!(bytes.len() as u64 <= SESSION_MAX_BYTES);
    assert_eq!(bytes.len() as u64, before + SESSION_CAP_MARKER.len() as u64);
    assert!(bytes.ends_with(SESSION_CAP_MARKER));
}
