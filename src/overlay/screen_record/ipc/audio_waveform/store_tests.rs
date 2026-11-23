use super::*;
use std::sync::{Arc, Barrier};

struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new() -> Self {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sgt-waveform-test-{:016x}",
            u64::from_le_bytes(nonce)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self) -> std::path::PathBuf {
        self.0.join("entry.bin")
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn envelope() -> SourceWaveformEnvelope {
    SourceWaveformEnvelope {
        source_duration_sec: 2.0 / 240.0,
        bins: vec![
            QuantizedWaveformBin { min: -10, max: 20 },
            QuantizedWaveformBin { min: -30, max: 40 },
        ],
    }
}

fn encoded(count: u32, duration: f64, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(WAVEFORM_CACHE_MAGIC);
    bytes
        .write_u32::<LittleEndian>(WAVEFORM_CACHE_VERSION)
        .unwrap();
    bytes.write_f64::<LittleEndian>(duration).unwrap();
    bytes.write_u32::<LittleEndian>(count).unwrap();
    bytes.extend_from_slice(data);
    bytes
}

#[test]
fn valid_cache_roundtrips_and_skips_analysis() {
    let dir = TestDir::new();
    let path = dir.file();
    save(&path, &envelope(), false).unwrap();
    let (cached, read) =
        load_or_analyze(Some(&path), || panic!("cache hit must not analyze")).unwrap();
    assert_eq!(read, CacheRead::Hit);
    assert_eq!(cached.bins.len(), 2);
    assert_eq!((cached.bins[1].min, cached.bins[1].max), (-30, 40));
}

#[test]
fn truncated_cache_is_reanalyzed_and_replaced_once() {
    let dir = TestDir::new();
    let path = dir.file();
    let invalid = encoded(2, 2.0 / 240.0, &[1, 2]);
    fs::write(&path, &invalid).unwrap();
    let (result, read) = load_or_analyze(Some(&path), || Ok(envelope())).unwrap();
    assert_eq!(read, CacheRead::Corrupt);
    assert_eq!(result.bins.len(), 2);
    // A plain publication still never overwrites an existing entry.
    assert!(save(&path, &result, false).is_err());
    assert_eq!(fs::read(&path).unwrap(), invalid);
    save(&path, &result, true).unwrap();
    let (cached, read) =
        load_or_analyze(Some(&path), || panic!("repaired cache must not analyze")).unwrap();
    assert_eq!(read, CacheRead::Hit);
    assert_eq!(cached.bins.len(), 2);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn foreign_format_entry_is_a_miss_not_replaced() {
    let dir = TestDir::new();
    let path = dir.file();
    let mut foreign = b"OTHER".to_vec();
    foreign.resize(32, 7);
    fs::write(&path, &foreign).unwrap();
    let (_, read) = load_or_analyze(Some(&path), || Ok(envelope())).unwrap();
    assert_eq!(read, CacheRead::Missing);
    assert_eq!(fs::read(&path).unwrap(), foreign);
}

#[test]
fn rejects_truncation_unbounded_count_bad_duration_and_bins() {
    let dir = TestDir::new();
    let path = dir.file();
    for bytes in [
        vec![1, 2],
        encoded(u32::MAX, 0.0, &[]),
        encoded(1, f64::NAN, &[0, 0, 0, 0]),
        encoded(1, -1.0, &[0, 0, 0, 0]),
        encoded(1, 1.0 / 240.0, &[1, 0, 0, 0]),
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
    }
}

#[test]
fn missing_or_unreadable_cache_does_not_hide_analysis_error() {
    let dir = TestDir::new();
    assert_eq!(
        load_or_analyze(Some(&dir.file()), || Err("decode failed".to_owned())).unwrap_err(),
        "decode failed"
    );
    let (value, read) = load_or_analyze(Some(&dir.0), || Ok(envelope())).unwrap();
    assert_eq!(read, CacheRead::Missing);
    assert_eq!(value.bins.len(), 2);
}

#[test]
fn concurrent_publications_preserve_the_winner() {
    let dir = TestDir::new();
    let path = dir.file();
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                save(&path, &envelope(), false).is_ok()
            })
        })
        .collect();
    let successes = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .filter(|success| *success)
        .count();
    assert_eq!(successes, 1);
    assert_eq!(load(&path).unwrap().unwrap().bins.len(), 2);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
}
