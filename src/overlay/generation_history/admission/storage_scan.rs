use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use super::MAX_SCANNED_MANAGED_ENTRIES;

pub(super) struct ManagedFiles {
    pub(super) paths: HashMap<String, u64>,
    physical: HashMap<String, u64>,
}

impl ManagedFiles {
    pub(super) fn total_bytes(&self) -> u64 {
        self.physical
            .values()
            .copied()
            .fold(0_u64, u64::saturating_add)
    }
}

pub(super) fn scan_managed_files(roots: &[PathBuf]) -> Result<ManagedFiles, String> {
    let mut files = ManagedFiles {
        paths: HashMap::new(),
        physical: HashMap::new(),
    };
    let mut pending = VecDeque::new();
    for root in roots {
        for name in [
            "3d-generator",
            "vectors",
            "images",
            "creation-staging",
            "creation-source-snapshots",
            "creation-source-previews",
        ] {
            let directory = root.join(name);
            match std::fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.file_type().is_dir() && !is_reparse_point(&metadata) => {
                    pending.push_back(directory);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err("Creation storage could not be inspected.".to_string()),
            }
        }
    }
    let mut scanned = 0_usize;
    while let Some(directory) = pending.pop_front() {
        for entry in std::fs::read_dir(&directory)
            .map_err(|_| "Creation storage could not be inspected.".to_string())?
        {
            let entry =
                entry.map_err(|_| "Creation storage could not be inspected.".to_string())?;
            scanned += 1;
            if scanned > MAX_SCANNED_MANAGED_ENTRIES {
                return Err("Creation storage contains too many entries.".to_string());
            }
            let metadata = std::fs::symlink_metadata(entry.path())
                .map_err(|_| "Creation storage could not be inspected.".to_string())?;
            if is_reparse_point(&metadata) {
                continue;
            }
            if metadata.file_type().is_file() {
                add_file(&mut files, entry.path(), metadata.len())?;
            } else if metadata.file_type().is_dir() {
                pending.push_back(entry.path());
            }
        }
    }
    for root in roots {
        for name in [
            "active-creation-intents.json",
            "creation-deliveries.json",
            "creation-result-history.json",
        ] {
            let path = root.join(name);
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
                return Err("Creation storage could not be inspected.".to_string());
            }
            add_file(&mut files, path, metadata.len())?;
        }
    }
    Ok(files)
}

fn add_file(files: &mut ManagedFiles, path: PathBuf, size: u64) -> Result<(), String> {
    let identity = crate::overlay::creation_file_identity::from_path(&path)?;
    if files
        .physical
        .insert(identity, size)
        .is_some_and(|saved| saved != size)
    {
        return Err("Creation storage changed while it was inspected.".to_string());
    }
    files.paths.insert(path_key(&path), size);
    Ok(())
}

fn path_key(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_ascii_lowercase()
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
