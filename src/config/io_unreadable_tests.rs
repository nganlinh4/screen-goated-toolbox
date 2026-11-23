//! An existing configuration that cannot be decoded must be preserved before
//! the loader falls back to defaults, because the next save replaces the file
//! (`parity-fixtures/preset-system/persistence.json`, unreadable/invalid
//! documents). A file another process holds locked is retried, and if it
//! stays unreadable the session must not save defaults over it.

use super::read_guard::{READ_ATTEMPTS, refuse_if_unread, unread_path};
use super::{LoadedConfig, load_config_with};
use crate::config::Config;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

const ERROR_SHARING_VIOLATION: i32 = 32;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sgt-config-unreadable-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    fn config_with(&self, contents: &[u8]) -> PathBuf {
        let path = self.0.join("config_v3.json");
        std::fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

fn preserved_copies(scratch: &Scratch) -> Vec<Vec<u8>> {
    std::fs::read_dir(&scratch.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("config_v3.json.corrupt-"))
        })
        .map(|path| std::fs::read(path).unwrap())
        .collect()
}

/// Default configs differ only by freshly generated profile and block ids.
fn comparable(config: &Config) -> serde_json::Value {
    fn strip_ids(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("id");
                map.values_mut().for_each(strip_ids);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip_ids),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(config).unwrap();
    strip_ids(&mut value);
    value
}

/// Loads through an injected reader and reports how many reads it made.
fn load_counting(
    path: &Path,
    mut read: impl FnMut(&Path, usize) -> io::Result<Vec<u8>>,
) -> (LoadedConfig, usize) {
    let mut reads = 0;
    let loaded = load_config_with(
        path,
        &mut |path| {
            reads += 1;
            read(path, reads)
        },
        Duration::ZERO,
    );
    (loaded, reads)
}

fn sharing_violation() -> io::Error {
    io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION)
}

#[test]
fn undecodable_existing_config_is_preserved_before_defaults_are_used() {
    let scratch = Scratch::new();
    // Valid JSON structure whose text is not UTF-8, as left by a damaged
    // sector or a foreign encoder; the user's keys are still recoverable.
    let original = [
        br#"{"api_key":"user-key-"#.as_slice(),
        &[0xff, 0xfe],
        br#""}"#,
    ]
    .concat();
    let path = scratch.config_with(&original);

    let (loaded, _) = load_counting(&path, |path, _| std::fs::read(path));

    assert_eq!(comparable(&loaded.config), comparable(&Config::default()));
    assert!(!loaded.disk_unread, "a preserved copy makes saving safe");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(preserved_copies(&scratch), vec![original]);
}

#[test]
fn briefly_locked_config_is_retried_and_loaded() {
    let scratch = Scratch::new();
    let path = scratch.config_with(br#"{"api_key":"user-key"}"#);

    let (loaded, reads) = load_counting(&path, |path, attempt| {
        if attempt < 3 {
            Err(sharing_violation())
        } else {
            std::fs::read(path)
        }
    });

    assert_eq!(reads, 3);
    assert_eq!(loaded.config.api_key, "user-key");
    assert!(!loaded.disk_unread);
    assert!(preserved_copies(&scratch).is_empty());
}

#[test]
fn config_that_stays_locked_is_never_replaced_by_defaults() {
    let scratch = Scratch::new();
    let original = br#"{"api_key":"user-key"}"#.to_vec();
    let path = scratch.config_with(&original);

    let (loaded, reads) = load_counting(&path, |_, _| Err(sharing_violation()));

    assert_eq!(reads, READ_ATTEMPTS);
    assert_eq!(comparable(&loaded.config), comparable(&Config::default()));
    assert!(loaded.disk_unread);
    let unread = AtomicBool::new(loaded.disk_unread);
    assert!(refuse_if_unread(&unread).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn permanent_read_errors_are_not_retried_and_still_block_saves() {
    let scratch = Scratch::new();
    let path = scratch.config_with(br#"{"api_key":"user-key"}"#);

    let (loaded, reads) = load_counting(&path, |_, _| {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    });

    assert_eq!(reads, 1);
    assert!(loaded.disk_unread);
}

#[test]
fn missing_config_starts_from_defaults_and_may_be_saved() {
    let scratch = Scratch::new();
    let path = scratch.0.join("config_v3.json");

    let (loaded, reads) = load_counting(&path, |path, _| std::fs::read(path));

    assert_eq!(reads, 1);
    assert!(!loaded.disk_unread);
    assert!(refuse_if_unread(&AtomicBool::new(loaded.disk_unread)).is_ok());
}

#[test]
fn an_unread_config_reports_its_path_for_the_session_warning() {
    let path = || PathBuf::from("config_v3.json");
    assert_eq!(
        unread_path(&AtomicBool::new(true), path),
        Some(PathBuf::from("config_v3.json"))
    );
    assert_eq!(unread_path(&AtomicBool::new(false), path), None);
}
