use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    CursorCollectionSpec, PointerCollectionSummary, cache_root, catalog_collections,
    downloading_ids, is_collection_complete,
};

#[derive(Clone, Copy)]
struct PointerSummaryDiskSnapshot {
    downloaded_count: usize,
    total_count: usize,
    downloaded_bytes: u64,
}

struct PointerSummaryCache {
    last_scan: Option<Instant>,
    snapshot: PointerSummaryDiskSnapshot,
    scanning: bool,
}

const SUMMARY_CACHE_TTL: Duration = Duration::from_millis(1500);

fn summary_cache() -> &'static Mutex<PointerSummaryCache> {
    static CACHE: OnceLock<Mutex<PointerSummaryCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(PointerSummaryCache {
            last_scan: None,
            snapshot: PointerSummaryDiskSnapshot {
                downloaded_count: 0,
                total_count: catalog_collections().len(),
                downloaded_bytes: 0,
            },
            scanning: false,
        })
    })
}

pub(super) fn invalidate_summary_cache() {
    if let Ok(mut cache) = summary_cache().lock() {
        cache.last_scan = None;
        cache.scanning = false;
    }
}

pub(crate) fn pointer_collection_summary() -> PointerCollectionSummary {
    let downloading_count = downloading_ids().lock().unwrap().len();
    let now = Instant::now();
    let root = cache_root();
    let snapshot = {
        let mut cache = summary_cache().lock().unwrap();
        let needs_refresh = cache
            .last_scan
            .is_none_or(|last| now.duration_since(last) >= SUMMARY_CACHE_TTL);
        if needs_refresh && !cache.scanning {
            cache.scanning = true;
            let root_for_thread = root.clone();
            thread::spawn(move || {
                let snapshot = compute_pointer_summary_disk(&root_for_thread);
                if let Ok(mut cache) = summary_cache().lock() {
                    cache.snapshot = snapshot;
                    cache.last_scan = Some(Instant::now());
                    cache.scanning = false;
                }
            });
        }
        cache.snapshot
    };

    PointerCollectionSummary {
        downloaded_count: snapshot.downloaded_count,
        total_count: snapshot.total_count,
        downloading_count,
        downloaded_bytes: snapshot.downloaded_bytes,
    }
}

fn compute_pointer_summary_disk(root: &Path) -> PointerSummaryDiskSnapshot {
    let mut downloaded_count = 0usize;
    let mut downloaded_bytes = 0u64;
    for spec in catalog_collections().iter().copied() {
        downloaded_bytes += collection_downloaded_size(spec, root);
        if is_collection_complete(spec, root) {
            downloaded_count += 1;
        }
    }

    PointerSummaryDiskSnapshot {
        downloaded_count,
        total_count: catalog_collections().len(),
        downloaded_bytes,
    }
}

pub(crate) fn scan_downloaded_collections() -> std::io::Result<PointerCollectionSummary> {
    let root = cache_root();
    ensure_directory_or_missing(&root)?;
    let mut downloaded_count = 0;
    let mut downloaded_bytes = 0u64;
    let mut remaining = 100_000usize;
    for spec in catalog_collections().iter().copied() {
        let directory = spec.local_dir(&root);
        if let Some(store) = directory.parent() {
            ensure_directory_or_missing(store)?;
        }
        ensure_directory_or_missing(&directory)?;
        let mut complete = true;
        for name in super::REQUIRED_FILES {
            match fs::symlink_metadata(directory.join(name)) {
                Ok(meta) if meta.file_type().is_file() && !is_reparse(&meta) => {}
                Ok(_) => return Err(std::io::Error::other("unsafe pointer collection entry")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => complete = false,
                Err(error) => return Err(error),
            }
        }
        if complete {
            downloaded_count += 1;
        }
        // Unverified packs from before source pinning still occupy disk until
        // Clean All removes them, so they count toward the reported size.
        let legacy = spec.legacy_dir(&root);
        ensure_directory_or_missing(&legacy)?;
        downloaded_bytes = downloaded_bytes
            .saturating_add(bounded_dir_size(&directory, 0, &mut remaining)?)
            .saturating_add(bounded_dir_size(&legacy, 0, &mut remaining)?);
    }
    Ok(PointerCollectionSummary {
        downloaded_count,
        total_count: catalog_collections().len(),
        downloading_count: downloading_ids().lock().unwrap().len(),
        downloaded_bytes,
    })
}

pub(super) fn ensure_directory_or_missing(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() && !is_reparse(&meta) => Ok(()),
        Ok(_) => Err(std::io::Error::other("unsafe pointer collection directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn bounded_dir_size(path: &Path, depth: usize, remaining: &mut usize) -> std::io::Result<u64> {
    if depth > 32 {
        return Err(std::io::Error::other("pointer collection is too deep"));
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut total = 0u64;
    for entry in entries {
        *remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| std::io::Error::other("pointer collection is too large"))?;
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if is_reparse(&meta) {
            return Err(std::io::Error::other("unsafe pointer collection entry"));
        } else if meta.file_type().is_dir() {
            total = total.saturating_add(bounded_dir_size(&entry.path(), depth + 1, remaining)?);
        } else if meta.file_type().is_file() {
            total = total.saturating_add(meta.len());
        } else {
            return Err(std::io::Error::other("unsafe pointer collection entry"));
        }
    }
    Ok(total)
}

#[cfg(target_os = "windows")]
pub(super) fn is_reparse(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(target_os = "windows"))]
pub(super) fn is_reparse(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

fn collection_downloaded_size(spec: CursorCollectionSpec, cache_root: &Path) -> u64 {
    dir_size_bytes(&spec.local_dir(cache_root)) + dir_size_bytes(&spec.legacy_dir(cache_root))
}

fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };

    let mut total = 0u64;
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                total += dir_size_bytes(&entry.path());
            } else {
                total += meta.len();
            }
        }
    }

    total
}

#[cfg(test)]
mod bounded_tests {
    use super::*;

    #[test]
    fn bounded_size_counts_owned_files_and_rejects_exhausted_budget() {
        let name = format!(
            "sgt-pack-scan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(name);
        fs::create_dir(&root).unwrap();
        fs::write(root.join("sample"), b"abc").unwrap();
        assert!(ensure_directory_or_missing(&root).is_ok());
        assert!(ensure_directory_or_missing(&root.join("missing")).is_ok());
        assert!(ensure_directory_or_missing(&root.join("sample")).is_err());
        assert_eq!(bounded_dir_size(&root, 0, &mut 2).unwrap(), 3);
        assert!(bounded_dir_size(&root, 0, &mut 0).is_err());
        assert_eq!(
            bounded_dir_size(&root.join("missing"), 0, &mut 2).unwrap(),
            0
        );
        fs::remove_file(root.join("sample")).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
