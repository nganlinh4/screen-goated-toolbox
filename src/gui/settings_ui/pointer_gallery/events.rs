//! Worker-lifecycle and state-reconciliation handlers for [`PointerGallery`].
//!
//! Extension `impl` block (mirroring `pointer_size.rs`) that keeps the render
//! method in `mod.rs` focused on UI. These methods own the background download
//! worker lifecycle, drain its events, reconcile collection status against the
//! on-disk file set, and apply a downloaded collection.

use super::backend::{
    apply_downloaded_collection, collection_specs, expected_file_count, is_collection_downloading,
    is_complete_files, required_file_names,
};
use super::worker::run_preload_worker;
use super::{
    CollectionState, CollectionStatus, DISK_SYNC_IDLE_INTERVAL_SECS, DISK_SYNC_INTERVAL_SECS,
    GalleryEvent, PointerGallery,
};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, TryRecvError};
use std::thread;

impl PointerGallery {
    pub(super) fn ensure_preload_started(&mut self) {
        if self.preload_started || self.downloads_paused {
            return;
        }

        self.stop_signal.store(false, Ordering::Relaxed);
        self.preload_started = true;
        self.status_message = None;
        for collection in &mut self.collections {
            collection.status = CollectionStatus::Queued;
            collection.files.clear();
            collection.file_order.clear();
        }
        self.preview_textures.clear();
        self.failed_previews.clear();

        let (tx, rx) = mpsc::channel();
        self.event_rx = Some(rx);
        let cache_root = self.cache_root.clone();
        let stop_signal = self.stop_signal.clone();

        thread::spawn(move || {
            run_preload_worker(&cache_root, tx, stop_signal);
        });
    }

    pub(super) fn restart_preload(&mut self) {
        self.stop_signal.store(false, Ordering::Relaxed);
        self.downloads_paused = false;
        self.preload_started = false;
        self.event_rx = None;
        self.status_message = None;
        self.ensure_preload_started();
    }

    pub(super) fn poll_worker_events(&mut self) {
        let mut disconnected = false;
        let mut pending_events = Vec::new();

        if let Some(rx) = &self.event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => pending_events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        for event in pending_events {
            self.apply_event(event);
        }

        if disconnected {
            self.event_rx = None;
            if self.downloads_paused {
                self.preload_started = false;
            }
        }
    }

    pub(super) fn sync_collections_from_disk(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        // While a download worker is active, poll the disk frequently so new
        // files surface quickly. Once it is idle, the file set is stable, so
        // back off to a much longer interval to avoid hundreds of redundant
        // stat() calls per second on the UI thread.
        let worker_active = self.event_rx.is_some();
        let interval = if worker_active {
            DISK_SYNC_INTERVAL_SECS
        } else {
            DISK_SYNC_IDLE_INTERVAL_SECS
        };
        if now - self.last_disk_sync_secs < interval {
            return;
        }
        self.last_disk_sync_secs = now;

        let total = expected_file_count();
        let mut removed_texture_keys = Vec::new();

        for collection in &mut self.collections {
            let mut files = HashMap::new();
            let mut order = Vec::new();
            let local_dir = collection.spec.local_dir(&self.cache_root);

            for file_name in required_file_names() {
                let path = local_dir.join(file_name);
                if path.exists() {
                    files.insert((*file_name).to_string(), path.clone());
                    order.push((*file_name).to_string());
                }
            }

            for old_path in collection.files.values() {
                if !files.values().any(|p| p == old_path) {
                    removed_texture_keys.push(old_path.to_string_lossy().to_string());
                }
            }

            collection.files = files;
            collection.file_order = order;

            let downloaded = collection.files.len();
            let downloading = is_collection_downloading(collection.spec.id);
            let keep_applied =
                matches!(collection.status, CollectionStatus::Applied) && downloaded >= total;
            if !keep_applied && !matches!(collection.status, CollectionStatus::Applying) {
                if downloaded >= total {
                    collection.status = CollectionStatus::Ready;
                } else if downloading {
                    collection.status = CollectionStatus::Downloading { downloaded, total };
                } else if downloaded > 0 || self.downloads_paused {
                    collection.status = CollectionStatus::Paused { downloaded, total };
                } else {
                    collection.status = CollectionStatus::Queued;
                }
            }
        }

        for key in removed_texture_keys {
            self.preview_textures.remove(&key);
            self.failed_previews.remove(&key);
            self.preview_loader.forget(&key);
        }
    }

    fn apply_event(&mut self, event: GalleryEvent) {
        match event {
            GalleryEvent::Progress {
                id,
                downloaded,
                total,
            } => {
                if let Some(collection) = self.collection_mut(&id) {
                    collection.status = CollectionStatus::Downloading { downloaded, total };
                }
            }
            GalleryEvent::FileReady {
                id,
                file_name,
                path,
            } => {
                if let Some(collection) = self.collection_mut(&id) {
                    collection.files.insert(file_name.clone(), path);
                    if !collection.file_order.iter().any(|name| name == &file_name) {
                        collection.file_order.push(file_name);
                    }
                }
            }
            GalleryEvent::Ready { id, files } => {
                if let Some(collection) = self.collection_mut(&id) {
                    let should_keep_applied =
                        matches!(collection.status, CollectionStatus::Applied);
                    collection.files = files.clone();
                    if collection.file_order.is_empty() {
                        let mut names: Vec<_> = files.keys().cloned().collect();
                        names.sort();
                        collection.file_order = names;
                    }
                    if !should_keep_applied {
                        if is_complete_files(&collection.files) {
                            collection.status = CollectionStatus::Ready;
                        } else {
                            collection.status = CollectionStatus::Paused {
                                downloaded: collection.files.len(),
                                total: expected_file_count(),
                            };
                        }
                    }
                }
            }
            GalleryEvent::Error { id, message } => {
                if let Some(collection) = self.collection_mut(&id) {
                    collection.status = CollectionStatus::Error(message);
                }
            }
            GalleryEvent::Paused => {
                self.downloads_paused = true;
                self.preload_started = false;
                for collection in &mut self.collections {
                    if matches!(
                        collection.status,
                        CollectionStatus::Queued | CollectionStatus::Downloading { .. }
                    ) {
                        collection.status = CollectionStatus::Paused {
                            downloaded: collection.files.len(),
                            total: expected_file_count(),
                        };
                    }
                }
            }
        }
    }

    fn collection_mut(&mut self, id: &str) -> Option<&mut CollectionState> {
        self.collections
            .iter_mut()
            .find(|entry| entry.spec.id == id)
    }

    pub(super) fn apply_collection(&mut self, id: &str, text: &LocaleText) {
        let Some(collection_idx) = self.collections.iter().position(|c| c.spec.id == id) else {
            return;
        };

        let status = self.collections[collection_idx].status.clone();
        if !matches!(status, CollectionStatus::Ready | CollectionStatus::Applied) {
            return;
        }

        self.collections[collection_idx].status = CollectionStatus::Applying;
        let spec = self.collections[collection_idx].spec;
        let files = self.collections[collection_idx].files.clone();

        match apply_downloaded_collection(spec, &files, self.pointer_size, false) {
            Ok(()) => {
                for entry in &mut self.collections {
                    if entry.spec.id == id {
                        entry.status = CollectionStatus::Applied;
                    } else if matches!(entry.status, CollectionStatus::Applied) {
                        entry.status = CollectionStatus::Ready;
                    }
                }
                self.status_message = Some((
                    true,
                    text.auxiliary
                        .managed_tools
                        .pointer_apply_success_fmt
                        .replace("{}", spec.title),
                ));
            }
            Err(err) => {
                self.collections[collection_idx].status = CollectionStatus::Error(err.clone());
                self.status_message = Some((false, err));
            }
        }
    }
}

/// Builds the initial per-collection state from the static collection specs.
pub(super) fn initial_collection_states() -> Vec<CollectionState> {
    collection_specs()
        .iter()
        .copied()
        .map(|spec| CollectionState {
            spec,
            status: CollectionStatus::Queued,
            files: HashMap::new(),
            file_order: Vec::new(),
        })
        .collect()
}
