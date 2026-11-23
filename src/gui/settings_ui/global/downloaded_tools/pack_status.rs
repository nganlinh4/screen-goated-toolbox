use eframe::egui;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::gui::settings_ui::pointer_gallery;
use crate::overlay::screen_record::bg_download;

const TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
pub(crate) struct PointerStatus {
    pub(crate) summary: pointer_gallery::PointerCollectionSummary,
    pub(crate) backup: bool,
}

struct Cache<T> {
    generation: u64,
    running: bool,
    value: Option<(Instant, T)>,
    retry_after: Option<Instant>,
}

impl<T: Copy> Cache<T> {
    fn new() -> Self {
        Self {
            generation: 0,
            running: false,
            value: None,
            retry_after: None,
        }
    }

    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.value = None;
        self.retry_after = None;
    }

    fn begin(&mut self) -> Option<u64> {
        if self.running
            || self
                .retry_after
                .is_some_and(|deadline| Instant::now() < deadline)
        {
            return None;
        }
        self.retry_after = None;
        self.running = true;
        Some(self.generation)
    }

    fn finish(&mut self, generation: u64, value: Option<T>) {
        self.running = false;
        if generation == self.generation {
            self.value = value.map(|value| (Instant::now(), value));
            self.retry_after = value.is_none().then(|| Instant::now() + TTL);
        }
    }
}

fn background_cache() -> &'static Mutex<Cache<bg_download::DownloadableBackgroundSummary>> {
    static CACHE: OnceLock<Mutex<Cache<bg_download::DownloadableBackgroundSummary>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::new()))
}

fn pointer_cache() -> &'static Mutex<Cache<PointerStatus>> {
    static CACHE: OnceLock<Mutex<Cache<PointerStatus>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::new()))
}

fn snapshot<T: Copy + Send + 'static>(
    cache: &'static Mutex<Cache<T>>,
    ctx: &egui::Context,
    scan: fn() -> std::io::Result<T>,
) -> Option<T> {
    let mut state = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some((updated, value)) = state.value
        && updated.elapsed() < TTL
    {
        ctx.request_repaint_after(TTL.saturating_sub(updated.elapsed()));
        return Some(value);
    }
    state.value = None;
    if let Some(generation) = state.begin() {
        let worker_ctx = ctx.clone();
        let launched = std::thread::Builder::new()
            .name("sgt-pack-status".into())
            .spawn(move || {
                let value = std::panic::catch_unwind(scan).ok().and_then(Result::ok);
                let failed = value.is_none();
                cache
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .finish(generation, value);
                worker_ctx.request_repaint();
                if failed {
                    worker_ctx.request_repaint_after(TTL);
                }
            });
        if launched.is_err() {
            state.finish(generation, None);
            ctx.request_repaint_after(TTL);
        }
    }
    None
}

pub(crate) fn backgrounds(
    ctx: &egui::Context,
) -> Option<bg_download::DownloadableBackgroundSummary> {
    snapshot(background_cache(), ctx, scan_backgrounds)
}

fn scan_backgrounds() -> std::io::Result<bg_download::DownloadableBackgroundSummary> {
    let root = crate::paths::app_runtime_local_data_dir().join("backgrounds");
    match std::fs::symlink_metadata(&root) {
        Ok(meta) if meta.file_type().is_dir() && !is_reparse(&meta) => {}
        Ok(_) => return Err(std::io::Error::other("unsafe background directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let items = bg_download::downloadable_backgrounds();
    let mut count = 0;
    let mut bytes = 0u64;
    for item in items {
        bg_download::validate_catalog_background_id(&item.id).map_err(std::io::Error::other)?;
        for ext in ["png", "jpg", "jpeg", "webp"] {
            match std::fs::symlink_metadata(root.join(format!("{}.{ext}", item.id))) {
                Ok(meta) if meta.file_type().is_file() && !is_reparse(&meta) => {
                    count += 1;
                    bytes = bytes.saturating_add(meta.len());
                    break;
                }
                Ok(_) => return Err(std::io::Error::other("unsafe background entry")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(bg_download::DownloadableBackgroundSummary {
        downloaded_count: count,
        total_count: items.len(),
        downloading_count: 0,
        downloaded_bytes: bytes,
    })
}

#[cfg(target_os = "windows")]
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(target_os = "windows"))]
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

pub(crate) fn pointers(ctx: &egui::Context) -> Option<PointerStatus> {
    snapshot(pointer_cache(), ctx, || {
        Ok(PointerStatus {
            summary: pointer_gallery::scan_downloaded_collections()?,
            backup: pointer_gallery::inspect_cursor_backup()?,
        })
    })
}

pub(crate) fn invalidate_backgrounds() {
    background_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .invalidate();
}

pub(crate) fn invalidate_pointers() {
    pointer_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .invalidate();
}

pub(crate) fn background_unavailable() -> bool {
    background_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retry_after
        .is_some()
}

pub(crate) fn pointer_unavailable() -> bool {
    pointer_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retry_after
        .is_some()
}

pub(crate) fn repaint() {
    let context = crate::gui::GUI_CONTEXT
        .lock()
        .ok()
        .and_then(|context| context.clone());
    if let Some(context) = context {
        context.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidation_rejects_old_scan_and_keeps_one_owner() {
        let mut cache = Cache::new();
        let old = cache.begin().unwrap();
        cache.invalidate();
        assert!(cache.begin().is_none());
        cache.finish(old, Some(10));
        assert!(cache.value.is_none());
        let current = cache.begin().unwrap();
        cache.finish(current, Some(20));
        assert_eq!(cache.value.map(|(_, value)| value), Some(20));
    }

    #[test]
    fn failed_scan_is_unknown_and_does_not_spin() {
        let mut cache = Cache::<u32>::new();
        let generation = cache.begin().unwrap();
        cache.finish(generation, None);
        assert!(cache.value.is_none());
        assert!(cache.retry_after.is_some());
        assert!(cache.begin().is_none());
        cache.invalidate();
        assert!(cache.begin().is_some());
    }
}
