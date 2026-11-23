use super::apply_worker::{ApplyOutcome, ApplyRequest, ApplyTarget, ApplyWorker};
use super::backend::{active_cursor_scheme, current_cursor_base_size};
use super::{CollectionState, CollectionStatus, LIVE_PREVIEW_APPLY_INTERVAL_SECS, PointerGallery};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Bumped by every successful restore, wherever it was started from, so an
/// open gallery stops treating its last pack as applied.
static RESTORE_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) fn restore_original_cursor() -> Result<(), String> {
    super::backend::restore_original_cursor_backup()?;
    RESTORE_GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

pub(super) fn restore_generation() -> u64 {
    RESTORE_GENERATION.load(Ordering::Relaxed)
}

/// The registry no longer shows any pack, so the size slider must not
/// re-apply one.
fn forget_applied(collections: &mut [CollectionState]) {
    for entry in collections {
        if matches!(entry.status, CollectionStatus::Applied) {
            entry.status = CollectionStatus::Ready;
        }
    }
}

/// Marks the pack whose scheme Windows reports as active. Returns false
/// while that pack's files are still being found on disk.
fn adopt_active_scheme(collections: &mut [CollectionState], active: Option<&str>) -> bool {
    let Some(active) = active else {
        return true;
    };
    let Some(entry) = collections
        .iter_mut()
        .find(|entry| entry.spec.scheme_name == active)
    else {
        return true;
    };
    match entry.status {
        CollectionStatus::Ready => {
            entry.status = CollectionStatus::Applied;
            true
        }
        CollectionStatus::Queued
        | CollectionStatus::Downloading { .. }
        | CollectionStatus::Paused { .. } => false,
        CollectionStatus::Applying | CollectionStatus::Applied | CollectionStatus::Error(_) => true,
    }
}

impl PointerGallery {
    pub(super) fn reconcile_with_restore(&mut self) {
        let generation = restore_generation();
        if generation == self.seen_restore_generation {
            return;
        }
        self.seen_restore_generation = generation;
        forget_applied(&mut self.collections);
        self.active_scheme_settled = false;
        self.pointer_size_loaded = false;
        self.last_live_apply_size = None;
    }

    /// Recovers which pack is applied after a restart, so the size slider
    /// re-scales that pack instead of only the system size.
    pub(super) fn adopt_applied_pack(&mut self) {
        if self.active_scheme_settled {
            return;
        }
        let active = active_cursor_scheme();
        self.active_scheme_settled = adopt_active_scheme(&mut self.collections, active.as_deref());
    }

    pub(super) fn ensure_pointer_size_loaded(&mut self) {
        if self.pointer_size_loaded {
            return;
        }
        if let Ok(size) = current_cursor_base_size() {
            self.pointer_size = size;
        }
        self.pointer_size_loaded = true;
    }

    pub(super) fn apply_pointer_size_live(
        &mut self,
        now: f64,
        live_preview_only: bool,
        force_apply: bool,
    ) {
        if !force_apply && self.last_live_apply_size == Some(self.pointer_size) {
            return;
        }
        if !force_apply
            && live_preview_only
            && now - self.last_live_apply_secs < LIVE_PREVIEW_APPLY_INTERVAL_SECS
        {
            return;
        }

        let target = self
            .collections
            .iter()
            .find(|entry| matches!(entry.status, CollectionStatus::Applied))
            .map_or(ApplyTarget::SystemSize, |entry| ApplyTarget::Pack {
                spec: entry.spec,
                files: entry.files.clone(),
                chosen: false,
            });
        self.submit_apply(ApplyRequest {
            target,
            size: self.pointer_size,
            live_preview_only,
        });
        self.last_live_apply_size = Some(self.pointer_size);
        self.last_live_apply_secs = now;
    }

    pub(super) fn submit_apply(&mut self, request: ApplyRequest) {
        self.apply_worker
            .get_or_insert_with(ApplyWorker::spawn)
            .submit(request);
    }

    pub(super) fn drain_apply_outcomes(&mut self, ctx: &egui::Context, text: &LocaleText) {
        let Some(worker) = &self.apply_worker else {
            return;
        };
        let mut outcomes = Vec::new();
        while let Some(outcome) = worker.try_outcome() {
            outcomes.push(outcome);
        }
        if worker.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        for outcome in outcomes {
            self.record_apply_outcome(outcome, text);
        }
    }

    fn record_apply_outcome(&mut self, outcome: ApplyOutcome, text: &LocaleText) {
        let ApplyOutcome {
            chosen_pack,
            result,
        } = outcome;
        match (chosen_pack, result) {
            (Some(spec), Ok(_)) => {
                for entry in &mut self.collections {
                    if entry.spec.id == spec.id {
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
            (Some(spec), Err(err)) => {
                if let Some(entry) = self
                    .collections
                    .iter_mut()
                    .find(|entry| entry.spec.id == spec.id)
                {
                    entry.status = CollectionStatus::Error(err.clone());
                }
                self.status_message = Some((false, err));
            }
            (None, Ok(_)) => {}
            (None, Err(err)) => self.status_message = Some((false, err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{adopt_active_scheme, forget_applied};
    use crate::gui::settings_ui::pointer_gallery::{CollectionStatus, events};

    #[test]
    fn a_restore_leaves_no_pack_for_the_size_slider_to_reapply() {
        let mut collections = events::initial_collection_states();
        collections[0].status = CollectionStatus::Applied;
        collections[1].status = CollectionStatus::Ready;
        forget_applied(&mut collections);
        assert!(
            collections
                .iter()
                .all(|entry| !matches!(entry.status, CollectionStatus::Applied))
        );
        assert!(matches!(collections[0].status, CollectionStatus::Ready));
    }

    #[test]
    fn the_scheme_windows_reports_marks_its_pack_once_found_on_disk() {
        let mut collections = events::initial_collection_states();
        let active = collections[2].spec.scheme_name;
        assert!(!adopt_active_scheme(&mut collections, Some(active)));
        collections[2].status = CollectionStatus::Ready;
        assert!(adopt_active_scheme(&mut collections, Some(active)));
        assert!(matches!(collections[2].status, CollectionStatus::Applied));
        assert!(adopt_active_scheme(
            &mut collections,
            Some("Windows Default")
        ));
        assert!(adopt_active_scheme(&mut collections, None));
    }
}
