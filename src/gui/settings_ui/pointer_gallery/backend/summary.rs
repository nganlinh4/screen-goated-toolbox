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

fn collection_downloaded_size(spec: CursorCollectionSpec, cache_root: &Path) -> u64 {
    dir_size_bytes(&spec.local_dir(cache_root))
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
