use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Localized, throttled ownership wrapper for component-download progress.
///
/// The badge is updated only when the displayed whole percent changes, and its
/// drop path only hides the notification if this download still owns it.
pub struct DownloadProgressBadge {
    title: String,
    message: String,
    last_percent: AtomicU32,
    finished: AtomicBool,
}

impl DownloadProgressBadge {
    pub fn new(component_name: &str) -> Self {
        let locale = super::locale_text();
        let title = super::format_locale(
            locale.downloading_component_fmt,
            &[("name", component_name)],
        );
        let message =
            super::format_locale(locale.preparing_component_fmt, &[("name", component_name)]);
        Self::with_text(&title, &message)
    }

    pub fn with_text(title: &str, message: &str) -> Self {
        super::show_progress_notification(title, message, 0.0);
        Self {
            title: title.to_string(),
            message: message.to_string(),
            last_percent: AtomicU32::new(0),
            finished: AtomicBool::new(false),
        }
    }

    pub fn report(&self, downloaded: u64, total: u64) {
        let percent = downloaded
            .saturating_mul(100)
            .checked_div(total.max(1))
            .unwrap_or(0)
            .min(100) as u32;
        if self.last_percent.swap(percent, Ordering::Relaxed) != percent {
            super::update_progress_notification_if_owned(
                &self.title,
                &self.message,
                percent as f32,
            );
        }
    }

    pub fn finish(&self) {
        if !self.finished.swap(true, Ordering::AcqRel) {
            super::hide_progress_notification_for(&self.title);
        }
    }
}

impl Drop for DownloadProgressBadge {
    fn drop(&mut self) {
        self.finish();
    }
}
