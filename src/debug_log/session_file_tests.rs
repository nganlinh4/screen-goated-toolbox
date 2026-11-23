use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sgt-debug-log-{label}-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn log_path(&self) -> PathBuf {
        self.0.join("session.log")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn oversized_lines_are_utf8_safe_and_bounded() {
    let line = "한".repeat(30_000);
    let bounded = bounded_line(&line, MAX_LOG_LINE_BYTES);

    assert!(bounded.len() <= MAX_LOG_LINE_BYTES);
    assert!(bounded.contains("[truncated; original_bytes=90000]"));
    assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
}

#[test]
fn compaction_keeps_recent_complete_lines_in_one_file() {
    let dir = TestDir::new("compaction");
    let path = dir.log_path();
    let content = (0..40)
        .map(|index| format!("line-{index:02}-abcdefghij\n"))
        .collect::<String>();
    std::fs::write(&path, content).unwrap();

    let compacted_bytes = compact_log(&path, 180).unwrap();
    let compacted = std::fs::read_to_string(&path).unwrap();

    assert!(compacted_bytes <= 180);
    assert!(compacted.starts_with(std::str::from_utf8(COMPACTION_MARKER).unwrap()));
    assert!(!compacted.contains("line-00"));
    assert!(compacted.contains("line-39"));
    assert!(!compaction_temp_path(&path).exists());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn writer_compacts_at_high_watermark_and_stays_bounded() {
    let dir = TestDir::new("watermark");
    let path = dir.log_path();
    let mut backoff = CompactionBackoff::default();
    let mut writer = CompactingWriter::open_with_limits(&path, 320, 200, &mut backoff).unwrap();

    for index in 0..20 {
        writer
            .write_line(&format!("entry-{index:02}-abcdefghijklmnop"))
            .unwrap();
    }
    writer.flush().unwrap();
    drop(writer);

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() <= 320);
    assert!(content.contains("Earlier entries removed"));
    assert!(content.contains("entry-19"));
    assert!(!compaction_temp_path(&path).exists());
}

/// A viewer that opens session.log without delete sharing makes ReplaceFileW
/// fail. Logging must continue past the cap and compact once it is released.
#[cfg(windows)]
#[test]
fn failed_compaction_keeps_appending_and_retries_after_backoff() {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;

    let dir = TestDir::new("deferred");
    let path = dir.log_path();
    std::fs::write(&path, "").unwrap();
    let viewer = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&path)
        .unwrap();
    let mut backoff = CompactionBackoff::default();
    let mut writer = CompactingWriter::open_with_limits(&path, 320, 200, &mut backoff).unwrap();
    for index in 0..20 {
        writer
            .write_line(&format!("entry-{index:02}-abcdefghijklmnop"))
            .expect("a failed compaction must not drop log lines");
    }
    writer.flush().unwrap();
    drop(writer);

    let content = std::fs::read_to_string(&path).unwrap();
    assert!((0..20).all(|index| content.contains(&format!("entry-{index:02}-"))));
    assert_eq!(content.matches("compaction deferred").count(), 1);
    assert!(std::fs::metadata(&path).unwrap().len() > 320);
    assert!(!compaction_temp_path(&path).exists());
    assert!(!backoff.ready(Instant::now()));

    drop(viewer);
    backoff.retry_at = Some(Instant::now());
    let mut writer = CompactingWriter::open_with_limits(&path, 320, 200, &mut backoff).unwrap();
    writer.write_line("entry-after-release").unwrap();
    writer.flush().unwrap();
    drop(writer);

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() <= 320);
    assert!(content.contains("Earlier entries removed"));
    assert!(content.contains("entry-after-release"));
    assert!(backoff.retry_at.is_none());
}

#[test]
fn compaction_backoff_doubles_up_to_its_cap() {
    let mut backoff = CompactionBackoff::default();
    let now = Instant::now();
    assert!(backoff.ready(now));
    assert_eq!(backoff.defer(now), COMPACTION_RETRY_FIRST);
    assert!(!backoff.ready(now));
    assert!(backoff.ready(now + COMPACTION_RETRY_FIRST));
    assert_eq!(backoff.defer(now), COMPACTION_RETRY_FIRST * 2);
    for _ in 0..16 {
        backoff.defer(now);
    }
    assert_eq!(backoff.defer(now), COMPACTION_RETRY_MAX);
}
