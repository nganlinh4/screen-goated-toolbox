use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SIZE_CACHE_TTL: Duration = Duration::from_secs(30);
const PROBE_CACHE_TTL: Duration = Duration::from_secs(2);
const VALUE_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
struct SizeCacheEntry {
    bytes: u64,
    updated_at: Option<Instant>,
    calculating: bool,
}

static SIZE_CACHE: OnceLock<Mutex<std::collections::HashMap<PathBuf, SizeCacheEntry>>> =
    OnceLock::new();
static PROBE_CACHE: OnceLock<Mutex<std::collections::HashMap<&'static str, ProbeCacheEntry>>> =
    OnceLock::new();
static U64_CACHE: OnceLock<Mutex<std::collections::HashMap<&'static str, U64CacheEntry>>> =
    OnceLock::new();

#[derive(Clone, Copy)]
struct ProbeCacheEntry {
    value: bool,
    updated_at: Option<Instant>,
    calculating: bool,
}

#[derive(Clone, Copy)]
struct U64CacheEntry {
    value: u64,
    updated_at: Option<Instant>,
    calculating: bool,
}

fn size_cache() -> &'static Mutex<std::collections::HashMap<PathBuf, SizeCacheEntry>> {
    SIZE_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn probe_cache() -> &'static Mutex<std::collections::HashMap<&'static str, ProbeCacheEntry>> {
    PROBE_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn u64_cache() -> &'static Mutex<std::collections::HashMap<&'static str, U64CacheEntry>> {
    U64_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

pub(super) fn cached_probe(
    key: &'static str,
    compute: impl FnOnce() -> bool + Send + 'static,
) -> bool {
    let now = Instant::now();

    if let Ok(mut cache) = probe_cache().lock() {
        let entry = cache.entry(key).or_insert(ProbeCacheEntry {
            value: false,
            updated_at: None,
            calculating: false,
        });
        let fresh = entry
            .updated_at
            .is_some_and(|updated_at| now.duration_since(updated_at) < PROBE_CACHE_TTL);
        if fresh || entry.calculating {
            return entry.value;
        }
        entry.calculating = true;
    }

    std::thread::spawn(move || {
        let value = compute();
        if let Ok(mut cache) = probe_cache().lock() {
            cache.insert(
                key,
                ProbeCacheEntry {
                    value,
                    updated_at: Some(Instant::now()),
                    calculating: false,
                },
            );
        }
    });

    probe_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).map(|entry| entry.value))
        .unwrap_or(false)
}

pub(super) fn invalidate_probe_cache(key: &'static str) {
    if let Ok(mut cache) = probe_cache().lock() {
        cache.remove(key);
    }
}

pub(super) fn cached_u64(key: &'static str, compute: impl FnOnce() -> u64 + Send + 'static) -> u64 {
    let now = Instant::now();

    if let Ok(mut cache) = u64_cache().lock() {
        let entry = cache.entry(key).or_insert(U64CacheEntry {
            value: 0,
            updated_at: None,
            calculating: false,
        });
        let fresh = entry
            .updated_at
            .is_some_and(|updated_at| now.duration_since(updated_at) < VALUE_CACHE_TTL);
        if fresh || entry.calculating {
            return entry.value;
        }
        entry.calculating = true;
    }

    std::thread::spawn(move || {
        let value = compute();
        if let Ok(mut cache) = u64_cache().lock() {
            cache.insert(
                key,
                U64CacheEntry {
                    value,
                    updated_at: Some(Instant::now()),
                    calculating: false,
                },
            );
        }
    });

    u64_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).map(|entry| entry.value))
        .unwrap_or(0)
}

pub(super) fn invalidate_u64_cache(key: &'static str) {
    if let Ok(mut cache) = u64_cache().lock() {
        cache.remove(key);
    }
}

pub(super) fn clear_downloaded_tools_caches() {
    if let Ok(mut cache) = size_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = probe_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = u64_cache().lock() {
        cache.clear();
    }
}

pub(super) fn get_dir_size(path: &Path) -> u64 {
    cached_size(path, true)
}

pub(super) fn get_path_size(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => cached_size(path, true),
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    }
}

pub(super) fn invalidate_size_cache(path: &Path) {
    if let Ok(mut cache) = size_cache().lock() {
        cache.remove(path);
    }
}

fn cached_size(path: &Path, recursive: bool) -> u64 {
    let path = path.to_path_buf();
    let now = Instant::now();

    if let Ok(mut cache) = size_cache().lock() {
        let entry = cache.entry(path.clone()).or_insert(SizeCacheEntry {
            bytes: 0,
            updated_at: None,
            calculating: false,
        });
        let fresh = entry
            .updated_at
            .is_some_and(|updated_at| now.duration_since(updated_at) < SIZE_CACHE_TTL);
        if fresh || entry.calculating {
            return entry.bytes;
        }
        entry.calculating = true;
    }

    let compute_path = path.clone();
    std::thread::spawn(move || {
        let bytes = if recursive {
            compute_dir_size(&compute_path)
        } else {
            fs::metadata(&compute_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        };
        if let Ok(mut cache) = size_cache().lock() {
            cache.insert(
                compute_path,
                SizeCacheEntry {
                    bytes,
                    updated_at: Some(Instant::now()),
                    calculating: false,
                },
            );
        }
    });

    size_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&path).map(|entry| entry.bytes))
        .unwrap_or(0)
}

fn compute_dir_size(path: &Path) -> u64 {
    let mut total_size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    total_size += compute_dir_size(&entry.path());
                } else {
                    total_size += metadata.len();
                }
            }
        }
    }
    total_size
}

pub(super) fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    format!("{:.1} MB", mb)
}
