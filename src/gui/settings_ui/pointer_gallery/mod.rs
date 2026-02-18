mod backend;
mod pointer_size;
mod preview;
mod worker;

use crate::gui::locale::LocaleText;
use backend::{
    apply_downloaded_collection, collection_specs, cursor_base_size_bounds,
    delete_all_downloaded_collections, expected_file_count, has_original_cursor_backup,
    is_collection_downloading, is_complete_files, new_stop_signal, pointer_collection_summary,
    required_file_names, restore_original_cursor_backup, start_download_all_missing,
    CursorCollectionSpec, PointerCollectionSummary,
};
use eframe::egui;
use preview::{
    preview_strip_width, render_collection_previews, render_status_label, PreviewTexture,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;
use worker::run_preload_worker;

const TITLE_COLUMN_WIDTH: f32 = 280.0;
const PREVIEW_ICON_SIZE: f32 = 24.0;
const ACTION_COLUMN_WIDTH: f32 = 104.0;
const STATUS_COLUMN_WIDTH: f32 = 120.0;
const DISK_SYNC_INTERVAL_SECS: f64 = 1.2;
const POINTER_SIZE_SLIDER_WIDTH: f32 = 180.0;
const DEFAULT_POINTER_SIZE: u32 = 32;
const LIVE_PREVIEW_APPLY_INTERVAL_SECS: f64 = 0.09;

#[derive(Clone)]
pub(super) enum CollectionStatus {
    Queued,
    Downloading { downloaded: usize, total: usize },
    Paused { downloaded: usize, total: usize },
    Ready,
    Applying,
    Applied,
    Error(String),
}

pub(super) struct CollectionState {
    spec: CursorCollectionSpec,
    status: CollectionStatus,
    files: HashMap<String, PathBuf>,
    file_order: Vec<String>,
}

pub(super) enum GalleryEvent {
    Progress {
        id: String,
        downloaded: usize,
        total: usize,
    },
    FileReady {
        id: String,
        file_name: String,
        path: PathBuf,
    },
    Ready {
        id: String,
        files: HashMap<String, PathBuf>,
    },
    Error {
        id: String,
        message: String,
    },
    Paused,
}

pub struct PointerGallery {
    pub show_window: bool,
    cache_root: PathBuf,
    collections: Vec<CollectionState>,
    preload_started: bool,
    downloads_paused: bool,
    last_disk_sync_secs: f64,
    event_rx: Option<Receiver<GalleryEvent>>,
    status_message: Option<(bool, String)>,
    preview_textures: HashMap<String, PreviewTexture>,
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pointer_size: u32,
    pointer_size_loaded: bool,
    last_live_apply_size: Option<u32>,
    last_live_apply_secs: f64,
}

pub(crate) fn downloadable_collection_summary() -> PointerCollectionSummary {
    pointer_collection_summary()
}

pub(crate) fn start_download_all_collections() -> usize {
    start_download_all_missing()
}

pub(crate) fn delete_downloaded_collections() -> usize {
    delete_all_downloaded_collections()
}

pub(crate) fn has_original_cursor_backup_file() -> bool {
    has_original_cursor_backup()
}

pub(crate) fn restore_original_cursor_from_backup() -> Result<(), String> {
    restore_original_cursor_backup()
}

impl PointerGallery {
    pub fn new() -> Self {
        let cache_root = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("screen-goated-toolbox")
            .join("pointer-gallery");

        let collections = collection_specs()
            .iter()
            .copied()
            .map(|spec| CollectionState {
                spec,
                status: CollectionStatus::Queued,
                files: HashMap::new(),
                file_order: Vec::new(),
            })
            .collect();

        Self {
            show_window: false,
            cache_root,
            collections,
            preload_started: false,
            downloads_paused: false,
            last_disk_sync_secs: 0.0,
            event_rx: None,
            status_message: None,
            preview_textures: HashMap::new(),
            stop_signal: new_stop_signal(),
            pointer_size: DEFAULT_POINTER_SIZE,
            pointer_size_loaded: false,
            last_live_apply_size: None,
            last_live_apply_secs: 0.0,
        }
    }

    pub fn render(&mut self, ctx: &egui::Context, text: &LocaleText) {
        if !self.show_window {
            return;
        }

        self.ensure_pointer_size_loaded();
        self.ensure_preload_started();
        self.poll_worker_events();
        self.sync_collections_from_disk(ctx);

        let mut open = true;
        let mut pending_apply: Option<String> = None;
        let mut retry_requested = false;

        egui::Window::new(text.pointer_gallery_btn)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(1180.0, 560.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let summary = pointer_collection_summary();
                    let can_restore = has_original_cursor_backup();
                    if ui
                        .add_enabled(
                            can_restore,
                            egui::Button::new(text.pointer_restore_original_btn),
                        )
                        .clicked()
                    {
                        match restore_original_cursor_backup() {
                            Ok(()) => {
                                self.pointer_size_loaded = false;
                                self.ensure_pointer_size_loaded();
                                self.status_message =
                                    Some((true, text.pointer_restore_success.to_string()));
                            }
                            Err(err) => {
                                self.status_message = Some((false, err));
                            }
                        }
                    }

                    let worker_running = self.event_rx.is_some() && !self.downloads_paused;
                    if worker_running && ui.button(text.pointer_action_stop).clicked() {
                        self.stop_signal.store(true, Ordering::Relaxed);
                        self.downloads_paused = true;
                        self.status_message =
                            Some((true, text.pointer_download_paused.to_string()));
                    }

                    if self.downloads_paused && ui.button(text.pointer_action_resume).clicked() {
                        self.downloads_paused = false;
                        self.restart_preload();
                    }

                    let has_missing = summary.downloaded_count < summary.total_count;
                    if has_missing
                        && !worker_running
                        && !self.downloads_paused
                        && ui.button(text.pointer_action_start_download).clicked()
                    {
                        self.restart_preload();
                    }

                    let (min_size, max_size) = cursor_base_size_bounds();
                    ui.separator();
                    ui.label(text.pointer_size_label);
                    let mut size_value = self.pointer_size.clamp(min_size, max_size);
                    let slider_response = ui.add_sized(
                        [POINTER_SIZE_SLIDER_WIDTH, PREVIEW_ICON_SIZE - 2.0],
                        egui::Slider::new(&mut size_value, min_size..=max_size).show_value(true),
                    );
                    if slider_response.changed() {
                        self.pointer_size = size_value;
                        let now = ctx.input(|i| i.time);
                        let live_preview_only = slider_response.dragged();
                        self.apply_pointer_size_live(now, live_preview_only, false);
                        if live_preview_only {
                            ctx.request_repaint_after(Duration::from_millis(80));
                        }
                    }
                    if slider_response.drag_stopped() {
                        let now = ctx.input(|i| i.time);
                        self.apply_pointer_size_live(now, false, true);
                    }
                });

                if let Some((is_ok, message)) = &self.status_message {
                    let color = if *is_ok {
                        egui::Color32::from_rgb(34, 139, 34)
                    } else {
                        egui::Color32::from_rgb(205, 92, 92)
                    };
                    ui.label(egui::RichText::new(message).color(color));
                }

                ui.add_space(8.0);

                let preview_width = preview_strip_width(expected_file_count(), PREVIEW_ICON_SIZE);
                let row_height = PREVIEW_ICON_SIZE + 18.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show_rows(ui, row_height, self.collections.len(), |ui, row_range| {
                        for idx in row_range {
                            let collection = &self.collections[idx];
                            let status = collection.status.clone();
                            let can_apply = matches!(
                                collection.status,
                                CollectionStatus::Ready | CollectionStatus::Applied
                            );
                            let apply_label =
                                if matches!(collection.status, CollectionStatus::Applied) {
                                    text.pointer_status_applied
                                } else {
                                    text.pointer_action_apply
                                };

                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    let title = egui::Label::new(
                                        egui::RichText::new(collection.spec.title).strong(),
                                    )
                                    .truncate();
                                    ui.add_sized(
                                        [TITLE_COLUMN_WIDTH, PREVIEW_ICON_SIZE + 8.0],
                                        title,
                                    )
                                    .on_hover_text(collection.spec.title);

                                    ui.allocate_ui_with_layout(
                                        egui::vec2(preview_width, PREVIEW_ICON_SIZE + 8.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            render_collection_previews(
                                                ui,
                                                ctx,
                                                collection,
                                                &mut self.preview_textures,
                                                required_file_names(),
                                                PREVIEW_ICON_SIZE,
                                            );
                                        },
                                    );

                                    if matches!(collection.status, CollectionStatus::Error(_)) {
                                        if ui
                                            .add_sized(
                                                [ACTION_COLUMN_WIDTH, PREVIEW_ICON_SIZE - 2.0],
                                                egui::Button::new(text.pointer_action_retry),
                                            )
                                            .clicked()
                                        {
                                            retry_requested = true;
                                        }
                                    } else {
                                        let clicked = ui
                                            .add_enabled_ui(can_apply, |ui| {
                                                ui.add_sized(
                                                    [ACTION_COLUMN_WIDTH, PREVIEW_ICON_SIZE - 2.0],
                                                    egui::Button::new(apply_label),
                                                )
                                            })
                                            .inner
                                            .clicked();
                                        if clicked {
                                            pending_apply = Some(collection.spec.id.to_string());
                                        }
                                    }

                                    let status_height = PREVIEW_ICON_SIZE + 8.0;
                                    let spacer_width =
                                        (ui.available_width() - STATUS_COLUMN_WIDTH).max(0.0);
                                    if spacer_width > 0.0 {
                                        ui.add_space(spacer_width);
                                    }

                                    ui.allocate_ui_with_layout(
                                        egui::vec2(STATUS_COLUMN_WIDTH, status_height),
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| render_status_label(ui, &status, text),
                                    );
                                });
                            });
                            ui.add_space(3.0);
                        }
                    });
            });

        self.show_window = open;

        if retry_requested {
            self.restart_preload();
        }

        if let Some(id) = pending_apply {
            self.apply_collection(&id, text);
        }
    }

    fn ensure_preload_started(&mut self) {
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

        let (tx, rx) = mpsc::channel();
        self.event_rx = Some(rx);
        let cache_root = self.cache_root.clone();
        let stop_signal = self.stop_signal.clone();

        thread::spawn(move || {
            run_preload_worker(&cache_root, tx, stop_signal);
        });
    }

    fn restart_preload(&mut self) {
        self.stop_signal.store(false, Ordering::Relaxed);
        self.downloads_paused = false;
        self.preload_started = false;
        self.event_rx = None;
        self.status_message = None;
        self.ensure_preload_started();
    }

    fn poll_worker_events(&mut self) {
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

    fn sync_collections_from_disk(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.last_disk_sync_secs < DISK_SYNC_INTERVAL_SECS {
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

    fn apply_collection(&mut self, id: &str, text: &LocaleText) {
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
                    text.pointer_apply_success_fmt.replace("{}", spec.title),
                ));
            }
            Err(err) => {
                self.collections[collection_idx].status = CollectionStatus::Error(err.clone());
                self.status_message = Some((false, err));
            }
        }
    }
}
