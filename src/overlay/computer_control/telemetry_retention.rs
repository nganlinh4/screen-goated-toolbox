use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_SESSIONS: usize = 64;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MINIMUM_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SESSIONS_SCANNED: usize = 4_096;
const MAX_TREE_ENTRIES: usize = 8_192;
const MAX_TREE_DEPTH: usize = 16;

struct Session {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
    removable: bool,
}

pub fn maintain(root: &Path) {
    maintain_with_limits(root, MAX_SESSIONS, MAX_BYTES, MINIMUM_AGE);
}

fn maintain_with_limits(root: &Path, max_sessions: usize, max_bytes: u64, minimum_age: Duration) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    let mut sessions = Vec::new();
    for entry in entries.flatten().take(MAX_SESSIONS_SCANNED) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("cc-") {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || metadata_is_reparse(&metadata) {
            continue;
        }
        let modified = metadata.modified().unwrap_or(now);
        let Some(bytes) = safe_tree_bytes(&path) else {
            continue;
        };
        sessions.push(Session {
            path,
            bytes,
            modified,
            removable: now
                .duration_since(modified)
                .is_ok_and(|age| age >= minimum_age),
        });
    }
    sessions.sort_by_key(|session| session.modified);
    let mut count = sessions.len();
    let mut bytes = sessions.iter().map(|session| session.bytes).sum::<u64>();
    for session in sessions {
        if count <= max_sessions && bytes <= max_bytes {
            break;
        }
        if session.removable && fs::remove_dir_all(&session.path).is_ok() {
            count = count.saturating_sub(1);
            bytes = bytes.saturating_sub(session.bytes);
        }
    }
}

fn safe_tree_bytes(root: &Path) -> Option<u64> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut scanned = 0_usize;
    let mut total = 0_u64;
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory).ok()? {
            scanned += 1;
            if scanned > MAX_TREE_ENTRIES {
                return None;
            }
            let entry = entry.ok()?;
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            if metadata_is_reparse(&metadata) {
                return None;
            }
            if metadata.is_dir() {
                if depth >= MAX_TREE_DEPTH {
                    return None;
                }
                pending.push((entry.path(), depth + 1));
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            } else {
                return None;
            }
        }
    }
    Some(total)
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_keeps_unknown_entries_and_newest_sessions() {
        let root = std::env::temp_dir().join(format!("sgt-cc-traces-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("user-note.txt"), b"keep").unwrap();
        for index in 0..4 {
            let session = root.join(format!("cc-{index}"));
            fs::create_dir_all(&session).unwrap();
            fs::write(session.join("events.jsonl"), vec![0_u8; index + 1]).unwrap();
        }

        maintain_with_limits(&root, 2, u64::MAX, Duration::ZERO);

        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("cc-"))
                .count(),
            2
        );
        assert!(root.join("user-note.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
