use crate::gui::theme::AppTheme;
use eframe::egui::{self, CornerRadius, Frame, Margin};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Themed Material container used for every downloaded-tools card, replacing the
/// default `ui.group(...)` so cards match the dialog surface (card fill, hairline
/// stroke, rounded corners, roomy padding).
pub(super) fn tool_card_frame(theme: &AppTheme) -> Frame {
    Frame::new()
        .fill(theme.card_bg())
        .stroke(theme.card_stroke())
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(crate::gui::theme::space::EDGE))
}

/// Render a tool card with the themed container, deriving the theme from `ui`.
pub(super) fn tool_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let theme = AppTheme::from_ui(ui);
    tool_card_frame(&theme).show(ui, add_contents);
}

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
static SIZE_CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);
static PROBE_CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);
static U64_CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);
static REMOVING: LazyLock<Mutex<std::collections::HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

pub(super) fn removal_in_progress(key: &'static str) -> bool {
    REMOVING
        .lock()
        .map(|removing| removing.contains(key))
        .unwrap_or(false)
}

pub(super) fn start_removal(
    key: &'static str,
    name: String,
    remove: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    after: impl FnOnce() + Send + 'static,
) {
    let started = REMOVING
        .lock()
        .map(|mut removing| removing.insert(key))
        .unwrap_or(false);
    if !started {
        return;
    }
    std::thread::spawn(move || {
        show_removal_started(&name);
        let result = remove();
        after();
        show_removal_finished(&name, &result);
        if let Ok(mut removing) = REMOVING.lock() {
            removing.remove(key);
        }
    });
}

fn show_removal_started(name: &str) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        locale.removing_component_fmt,
        &[("name", name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &title,
        "",
        crate::overlay::auto_copy_badge::NotificationType::Info,
    );
}

fn show_removal_finished(name: &str, result: &anyhow::Result<()>) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = match result {
        Ok(()) => (
            locale.component_removed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        ),
        Err(error) => (
            locale.component_remove_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        ),
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
}

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

fn request_gui_repaint() {
    let context = crate::gui::GUI_CONTEXT
        .lock()
        .ok()
        .and_then(|context| context.clone());
    if let Some(context) = context {
        context.request_repaint();
    }
}

pub(super) fn cached_probe(
    key: &'static str,
    compute: impl FnOnce() -> bool + Send + 'static,
) -> bool {
    let now = Instant::now();
    let generation = if let Ok(mut cache) = probe_cache().lock() {
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
        PROBE_CACHE_GENERATION.load(Ordering::Acquire)
    } else {
        PROBE_CACHE_GENERATION.load(Ordering::Acquire)
    };

    std::thread::spawn(move || {
        let value = compute();
        if let Ok(mut cache) = probe_cache().lock() {
            if PROBE_CACHE_GENERATION.load(Ordering::Acquire) != generation {
                return;
            }
            cache.insert(
                key,
                ProbeCacheEntry {
                    value,
                    updated_at: Some(Instant::now()),
                    calculating: false,
                },
            );
        }
        request_gui_repaint();
    });

    probe_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).map(|entry| entry.value))
        .unwrap_or(false)
}

pub(super) fn invalidate_probe_cache(_key: &'static str) {
    if let Ok(mut cache) = probe_cache().lock() {
        PROBE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    } else {
        PROBE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    request_gui_repaint();
}

pub(super) fn cached_u64(key: &'static str, compute: impl FnOnce() -> u64 + Send + 'static) -> u64 {
    let now = Instant::now();
    let generation = if let Ok(mut cache) = u64_cache().lock() {
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
        U64_CACHE_GENERATION.load(Ordering::Acquire)
    } else {
        U64_CACHE_GENERATION.load(Ordering::Acquire)
    };

    std::thread::spawn(move || {
        let value = compute();
        if let Ok(mut cache) = u64_cache().lock() {
            if U64_CACHE_GENERATION.load(Ordering::Acquire) != generation {
                return;
            }
            cache.insert(
                key,
                U64CacheEntry {
                    value,
                    updated_at: Some(Instant::now()),
                    calculating: false,
                },
            );
        }
        request_gui_repaint();
    });

    u64_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).map(|entry| entry.value))
        .unwrap_or(0)
}

pub(super) fn invalidate_u64_cache(_key: &'static str) {
    if let Ok(mut cache) = u64_cache().lock() {
        U64_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    } else {
        U64_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    request_gui_repaint();
}

pub(super) fn clear_downloaded_tools_caches() {
    if let Ok(mut cache) = size_cache().lock() {
        SIZE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    } else {
        SIZE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    if let Ok(mut cache) = probe_cache().lock() {
        PROBE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    } else {
        PROBE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    if let Ok(mut cache) = u64_cache().lock() {
        U64_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    } else {
        U64_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    request_gui_repaint();
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

pub(super) fn invalidate_size_cache(_path: &Path) {
    if let Ok(mut cache) = size_cache().lock() {
        SIZE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    } else {
        SIZE_CACHE_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    request_gui_repaint();
}

fn cached_size(path: &Path, recursive: bool) -> u64 {
    let path = path.to_path_buf();
    let now = Instant::now();
    let generation = if let Ok(mut cache) = size_cache().lock() {
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
        SIZE_CACHE_GENERATION.load(Ordering::Acquire)
    } else {
        SIZE_CACHE_GENERATION.load(Ordering::Acquire)
    };

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
            if SIZE_CACHE_GENERATION.load(Ordering::Acquire) != generation {
                return;
            }
            cache.insert(
                compute_path,
                SizeCacheEntry {
                    bytes,
                    updated_at: Some(Instant::now()),
                    calculating: false,
                },
            );
        }
        request_gui_repaint();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn completed_background_probe_wakes_ui_with_new_value() {
        const KEY: &str = "downloaded-tools:test-background-probe-repaint";

        invalidate_probe_cache(KEY);
        let context = egui::Context::default();
        let (sender, receiver) = mpsc::channel();
        context.set_request_repaint_callback(move |_| {
            let _ = sender.send(());
        });
        *crate::gui::GUI_CONTEXT.lock().unwrap() = Some(context);

        let initial = cached_probe(KEY, || true);
        let repainted = receiver.recv_timeout(Duration::from_secs(1)).is_ok();
        let updated = cached_probe(KEY, || false);

        *crate::gui::GUI_CONTEXT.lock().unwrap() = None;
        invalidate_probe_cache(KEY);

        assert!(!initial);
        assert!(repainted);
        assert!(updated);
    }
}
