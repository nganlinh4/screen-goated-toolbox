use super::*;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn fresh_dir(label: &str) -> PathBuf {
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).unwrap();
    let dir = crate::paths::app_temp_dir().join(format!(
        "cc-artifact-sweep-{label}-{}-{:016x}",
        std::process::id(),
        u64::from_le_bytes(nonce)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_aged(dir: &Path, name: &str, bytes: usize, modified: SystemTime) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, vec![b'x'; bytes]).unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    path
}

#[test]
fn stale_orphans_are_removed_while_live_foreign_and_young_files_survive() {
    let dir = fresh_dir("age");
    let now = SystemTime::now();
    let stale = write_aged(&dir, "art_1_old.txt", 10, now - 2 * DAY);
    let young = write_aged(&dir, "art_2_young.txt", 10, now - DAY / 2);
    let foreign = write_aged(&dir, "notes.txt", 10, now - 30 * DAY);
    let live = write_aged(&dir, "art_3_live.txt", 10, now - 30 * DAY);
    // A concurrent session holds its cache file open, as CacheFile does.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(
            windows::Win32::Storage::FileSystem::FILE_SHARE_READ.0
                | windows::Win32::Storage::FileSystem::FILE_SHARE_DELETE.0,
        )
        .open(&live)
        .unwrap();
    std::fs::create_dir(dir.join("art_4_dir.txt")).unwrap();

    let report = sweep(&dir, now, STARTUP_POLICY);

    assert_eq!(
        report,
        Report {
            removed: 1,
            in_use: 1,
            kept: 1,
        }
    );
    assert!(!stale.exists());
    assert!(young.exists());
    assert!(foreign.exists());
    assert!(live.exists());
    drop(held);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn young_orphans_beyond_the_byte_cap_are_removed_oldest_first() {
    let dir = fresh_dir("size");
    let now = SystemTime::now();
    let oldest = write_aged(&dir, "art_1_a.txt", 400, now - 3 * DAY / 4);
    let middle = write_aged(&dir, "art_2_b.txt", 400, now - DAY / 2);
    let newest = write_aged(&dir, "art_3_c.txt", 400, now - DAY / 4);
    let policy = Policy {
        max_age: DAY,
        max_bytes: 800,
    };

    let report = sweep(&dir, now, policy);

    assert_eq!(report.removed, 1);
    assert_eq!(report.kept, 2);
    assert!(!oldest.exists());
    assert!(middle.exists());
    assert!(newest.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_missing_cache_directory_is_not_created_or_reported() {
    let dir = crate::paths::app_temp_dir()
        .join(format!("cc-artifact-sweep-missing-{}", std::process::id()));
    assert_eq!(
        sweep(&dir, SystemTime::now(), STARTUP_POLICY),
        Report::default()
    );
    assert!(!dir.exists());
}

#[test]
fn a_live_session_cache_file_survives_the_sweep() {
    let dir = fresh_dir("cachefile");
    let path = dir.join("art_5_owned.txt");
    let owned = super::super::cache_file::CacheFile::create(&path, "live").unwrap();
    // Even far in the future (past the age limit), the held file is not touched.
    let report = sweep(&dir, SystemTime::now() + 30 * DAY, STARTUP_POLICY);
    assert_eq!(report.removed, 0);
    assert_eq!(owned.read(64).unwrap(), "live");
    drop(owned);
    assert!(!path.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
