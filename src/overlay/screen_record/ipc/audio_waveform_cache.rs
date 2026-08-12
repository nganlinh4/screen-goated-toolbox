use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILES: usize = 512;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

struct Entry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

pub fn maintain(root: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut managed = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("bin") {
                return None;
            }
            let metadata = fs::symlink_metadata(&path).ok()?;
            if !metadata.is_file() || metadata_is_reparse(&metadata) {
                return None;
            }
            Some(Entry {
                path,
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect::<Vec<_>>();
    managed.sort_by_key(|entry| entry.modified);
    let mut files = managed.len();
    let mut bytes = managed.iter().map(|entry| entry.bytes).sum::<u64>();
    for entry in managed {
        if files <= MAX_FILES && bytes <= MAX_BYTES {
            break;
        }
        if entry.path == current {
            continue;
        }
        if fs::remove_file(&entry.path).is_ok() {
            files = files.saturating_sub(1);
            bytes = bytes.saturating_sub(entry.bytes);
        }
    }
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
    fn retention_removes_only_old_managed_waveforms() {
        let root = std::env::temp_dir().join(format!("sgt-waveforms-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let unknown = root.join("keep.txt");
        fs::write(&unknown, b"user").unwrap();
        for index in 0..=MAX_FILES {
            fs::write(root.join(format!("{index:04}.bin")), b"cache").unwrap();
        }
        let current = root.join(format!("{MAX_FILES:04}.bin"));

        maintain(&root, &current);

        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().and_then(|value| value.to_str())
                    == Some("bin"))
                .count(),
            MAX_FILES
        );
        assert!(current.exists());
        assert!(unknown.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
