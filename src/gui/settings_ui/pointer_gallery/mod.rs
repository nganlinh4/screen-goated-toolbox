mod backend;
mod events;
mod pointer_size;
mod preview;
mod worker;

use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use backend::{
    CursorCollectionSpec, PointerCollectionSummary, cursor_base_size_bounds,
    delete_all_downloaded_collections, expected_file_count, has_original_cursor_backup,
    new_stop_signal, pointer_collection_summary, required_file_names,
    restore_original_cursor_backup, start_download_all_missing,
};
use eframe::egui;
use preview::{
    PreviewLoader, PreviewTexture, preview_strip_width, render_collection_previews,
    render_status_label,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const TITLE_COLUMN_WIDTH: f32 = 280.0;
const PREVIEW_ICON_SIZE: f32 = 24.0;
const ACTION_COLUMN_WIDTH: f32 = 104.0;
const STATUS_COLUMN_WIDTH: f32 = 120.0;
const DISK_SYNC_INTERVAL_SECS: f64 = 1.2;
const DISK_SYNC_IDLE_INTERVAL_SECS: f64 = 8.0;
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
    preview_loader: PreviewLoader,
    failed_previews: HashSet<String>,
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

impl Default for PointerGallery {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerGallery {
    pub fn new() -> Self {
        let cache_root = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("screen-goated-toolbox")
            .join("pointer-gallery");

        let collections = events::initial_collection_states();

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
            preview_loader: PreviewLoader::new(),
            failed_previews: HashSet::new(),
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

        // Upload any previews decoded on the background thread. Keep repainting
        // while decodes are still in flight so freshly-decoded icons appear.
        let now = ctx.input(|i| i.time);
        let previews_pending = self.preview_loader.drain(
            ctx,
            &mut self.preview_textures,
            &mut self.failed_previews,
            now,
        );
        if previews_pending {
            ctx.request_repaint_after(Duration::from_millis(33));
        }

        let mut open = true;
        let mut pending_apply: Option<String> = None;
        let mut retry_requested = false;

        let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);

        // Manual full-viewport scrim behind the (large, resizable) gallery window
        // so it reads as the clear focus, matching the modal dialog treatment.
        let screen_rect = ctx.content_rect();
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("pointer_gallery_scrim"),
        ))
        .rect_filled(screen_rect, 0.0, theme.scrim_color());

        egui::Window::new(text.pointer_gallery_btn)
            .collapsible(false)
            .resizable(true)
            .title_bar(false)
            .frame(theme.dialog_frame())
            .default_size(egui::vec2(900.0, 560.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let description = self
                    .status_message
                    .as_ref()
                    .map(|(_, message)| message.clone());
                let header_closed = crate::gui::widgets::dialog_header(
                    ui,
                    &theme,
                    text.pointer_gallery_btn,
                    description.as_deref(),
                    |ui| {
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

                        if self.downloads_paused && ui.button(text.pointer_action_resume).clicked()
                        {
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
                        // Group the size control away from the action buttons with
                        // spacing rather than a divider line (clean UI).
                        ui.add_space(10.0);
                        ui.label(text.pointer_size_label);
                        let mut size_value = self.pointer_size.clamp(min_size, max_size);
                        let slider_response = ui.add_sized(
                            [POINTER_SIZE_SLIDER_WIDTH, PREVIEW_ICON_SIZE - 2.0],
                            egui::Slider::new(&mut size_value, min_size..=max_size)
                                .show_value(true),
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
                    },
                );
                if header_closed {
                    open = false;
                }

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
                                                &mut self.preview_loader,
                                                &self.failed_previews,
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
}
