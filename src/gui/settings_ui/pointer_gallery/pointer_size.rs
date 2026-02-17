use super::backend::{
    apply_cursor_base_size, apply_downloaded_collection, current_cursor_base_size,
};
use super::{CollectionStatus, PointerGallery, LIVE_PREVIEW_APPLY_INTERVAL_SECS};

impl PointerGallery {
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

        let applied_collection = self
            .collections
            .iter()
            .find(|entry| matches!(entry.status, CollectionStatus::Applied))
            .map(|entry| (entry.spec, entry.files.clone()));

        let result = if let Some((spec, files)) = applied_collection {
            apply_downloaded_collection(spec, &files, self.pointer_size, live_preview_only)
                .map(|_| self.pointer_size)
        } else {
            apply_cursor_base_size(self.pointer_size)
        };

        match result {
            Ok(applied_size) => {
                self.pointer_size = applied_size;
                self.last_live_apply_size = Some(applied_size);
                self.last_live_apply_secs = now;
            }
            Err(err) => {
                self.status_message = Some((false, err));
            }
        }
    }
}
