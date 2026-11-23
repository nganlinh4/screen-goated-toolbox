use super::*;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let name = super::super::crypto::random_hex(16).unwrap();
        let path = std::env::temp_dir().join(format!("sgt-pairing-test-{name}"));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn key(&self) -> PathBuf {
        self.0.join("key")
    }
    fn assert_only_key(&self) {
        assert_eq!(std::fs::read_dir(&self.0).unwrap().count(), 1);
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.key());
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn new_key_is_published_and_reused_without_calling_entropy_again() {
    let directory = Directory::new();
    let key = load_or_make(&directory.key()).unwrap();
    assert!(valid(&key));
    assert_eq!(std::fs::read_to_string(directory.key()).unwrap(), key);
    assert_eq!(
        load_with(&directory.key(), || panic!("existing key must be reused")).unwrap(),
        key
    );
    directory.assert_only_key();
}

#[test]
fn invalid_oversized_and_unreadable_existing_keys_are_not_replaced() {
    let directory = Directory::new();
    for content in [b"invalid".to_vec(), vec![b'a'; 1024], vec![0xff; 48]] {
        std::fs::write(directory.key(), &content).unwrap();
        assert_eq!(
            load_with(&directory.key(), || panic!(
                "must preserve existing material"
            )),
            Err(Error::Invalid)
        );
        assert_eq!(std::fs::read(directory.key()).unwrap(), content);
    }
    let handle = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(directory.key())
        .unwrap();
    assert_eq!(
        load_with(&directory.key(), || panic!(
            "must not overwrite busy material"
        )),
        Err(Error::Unavailable)
    );
    drop(handle);
    directory.assert_only_key();
}

#[test]
fn entropy_failure_does_not_create_a_key_or_fallback_value() {
    let directory = Directory::new();
    assert_eq!(
        load_with(&directory.key(), || Err(Error::Entropy)),
        Err(Error::Entropy)
    );
    assert!(!directory.key().exists());
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn concurrent_creators_return_the_same_committed_key() {
    let directory = Directory::new();
    let barrier = Arc::new(Barrier::new(4));
    let workers = (0..4)
        .map(|_| {
            let path = directory.key();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                load_with(&path, || {
                    barrier.wait();
                    super::super::crypto::random_hex(24).map_err(|_| Error::Entropy)
                })
            })
        })
        .collect::<Vec<_>>();
    let values = workers
        .into_iter()
        .map(|worker| worker.join().unwrap().unwrap())
        .collect::<Vec<_>>();
    assert!(values.iter().all(|value| value == &values[0]));
    assert_eq!(std::fs::read_to_string(directory.key()).unwrap(), values[0]);
    directory.assert_only_key();
}

#[test]
fn candidate_cannot_replace_an_existing_invalid_winner() {
    let directory = Directory::new();
    let result = load_with(&directory.key(), || {
        std::fs::write(directory.key(), b"preserve").unwrap();
        Ok("ab".repeat(24))
    });
    assert_eq!(result, Err(Error::Invalid));
    assert_eq!(std::fs::read(directory.key()).unwrap(), b"preserve");
    directory.assert_only_key();
}

#[test]
fn directory_and_hardlinked_key_fail_without_changing_target() {
    let directory = Directory::new();
    std::fs::create_dir(directory.key()).unwrap();
    assert!(load_or_make(&directory.key()).is_err());
    std::fs::remove_dir(directory.key()).unwrap();
    let target = directory.0.join("target");
    let content = "ab".repeat(24);
    std::fs::write(&target, &content).unwrap();
    std::fs::hard_link(&target, directory.key()).unwrap();
    assert_eq!(load_or_make(&directory.key()), Err(Error::Invalid));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), content);
    std::fs::remove_file(directory.key()).unwrap();
    std::fs::remove_file(target).unwrap();
}

#[test]
fn missing_parent_is_created_but_existing_invalid_parent_is_preserved() {
    let directory = Directory::new();
    let parent = directory.0.join("nested");
    let key = parent.join("key");
    assert!(valid(&load_or_make(&key).unwrap()));
    std::fs::remove_file(key).unwrap();
    std::fs::remove_dir(&parent).unwrap();
    std::fs::write(&parent, b"preserve").unwrap();
    assert_eq!(load_or_make(&parent.join("key")), Err(Error::Unavailable));
    assert_eq!(std::fs::read(&parent).unwrap(), b"preserve");
    std::fs::remove_file(parent).unwrap();
}

#[test]
fn abandoned_scratch_is_removed_but_a_live_creators_scratch_is_kept() {
    let directory = Directory::new();
    let scratch = |id: &str| {
        directory
            .0
            .join(format!("{SCRATCH_PREFIX}{id}{SCRATCH_SUFFIX}"))
    };
    let abandoned = scratch(&"a".repeat(32));
    let live = scratch(&"b".repeat(32));
    let unrelated = directory.0.join(".sgt-pairing-notes.tmp");
    for path in [&abandoned, &live, &unrelated] {
        std::fs::write(path, b"partial").unwrap();
    }
    let held = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&live)
        .unwrap();
    assert!(valid(&load_or_make(&directory.key()).unwrap()));
    assert!(!abandoned.exists());
    assert!(live.exists() && unrelated.exists());
    drop(held);
    std::fs::remove_file(live).unwrap();
    std::fs::remove_file(unrelated).unwrap();
    directory.assert_only_key();
}

#[test]
fn the_former_deterministic_fallback_is_never_accepted_or_published() {
    let fallback: String = (0..24u8)
        .map(|index| format!("{:02x}", index.wrapping_mul(31).wrapping_add(7)))
        .collect();
    assert!(valid(&fallback) && publicly_derivable(&fallback));
    let directory = Directory::new();
    assert_eq!(
        load_with(&directory.key(), || Ok(fallback.clone())),
        Err(Error::Entropy)
    );
    assert!(!directory.key().exists());
    std::fs::write(directory.key(), &fallback).unwrap();
    assert_eq!(
        load_with(&directory.key(), || panic!(
            "must preserve existing material"
        )),
        Err(Error::Invalid)
    );
    assert_eq!(std::fs::read_to_string(directory.key()).unwrap(), fallback);
}
