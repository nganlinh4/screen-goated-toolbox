use super::{CleanupDialogState, set_state};
use eframe::egui;

/// A dropped worker, including spawn failure or unwind, must release the modal.
pub(super) struct CleanupCompletion {
    context: egui::Context,
    attention_count: usize,
    refresh: Option<Box<dyn FnOnce() + Send>>,
    started: bool,
    quiescence: Option<Box<dyn Send>>,
}

impl CleanupCompletion {
    pub(super) fn new(context: egui::Context, refresh: impl FnOnce() + Send + 'static) -> Self {
        Self {
            context,
            attention_count: 1,
            refresh: Some(Box::new(refresh)),
            started: false,
            quiescence: None,
        }
    }

    pub(super) fn mark_started(&mut self) {
        self.started = true;
    }

    pub(super) fn hold_quiescence(&mut self, guard: impl Send + 'static) {
        self.quiescence = Some(Box::new(guard));
    }

    pub(super) fn finish(&mut self, attention_count: usize) {
        self.attention_count = attention_count;
    }
}

impl Drop for CleanupCompletion {
    fn drop(&mut self) {
        if let Some(refresh) = self.refresh.take()
            && self.started
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(refresh)).is_err()
        {
            self.attention_count = self.attention_count.max(1);
        }
        let _ = std::panic::catch_unwind(super::super::utils::clear_downloaded_tools_caches);
        set_state(CleanupDialogState::Complete {
            attention_count: self.attention_count,
        });
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.context.request_repaint()
        }));
    }
}
