use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

const DORMANT: u8 = 0;
const ACTIVE: u8 = 1;
const FINISHED: u8 = 2;

/// Localized, throttled ownership wrapper for component-download progress.
///
/// The badge is updated only when the displayed whole percent changes, and its
/// drop path only hides the notification if this download still owns it.
pub struct DownloadProgressBadge {
    title: String,
    initial_message: String,
    progress_message: String,
    last_percent: AtomicU32,
    state: AtomicU8,
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
        Self::with_messages(&title, &message, "")
    }

    pub fn with_text(title: &str, message: &str) -> Self {
        Self::with_messages(title, message, message)
    }

    fn with_messages(title: &str, initial_message: &str, progress_message: &str) -> Self {
        Self {
            title: title.to_string(),
            initial_message: initial_message.to_string(),
            progress_message: progress_message.to_string(),
            last_percent: AtomicU32::new(u32::MAX),
            state: AtomicU8::new(DORMANT),
        }
    }

    pub fn report(&self, downloaded: u64, total: u64) {
        self.report_with_message(downloaded, total, &self.progress_message);
    }

    pub fn report_with_message(&self, downloaded: u64, total: u64, message: &str) {
        if !self.start() {
            return;
        }
        let percent = downloaded
            .saturating_mul(100)
            .checked_div(total.max(1))
            .unwrap_or(0)
            .min(100) as u32;
        if self.last_percent.swap(percent, Ordering::Relaxed) != percent {
            super::update_progress_notification_if_owned(&self.title, message, percent as f32);
        }
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub fn set_phase(&self, message: &str, progress: f32) {
        if !self.start() {
            return;
        }
        super::update_progress_notification_if_owned(&self.title, message, progress);
    }

    /// Make the progress surface visible once real component work has begun.
    ///
    /// Constructors are intentionally silent. Readiness checks commonly create
    /// a badge before discovering that the verified component is already
    /// installed; rendering at construction time made every open look like a
    /// fresh download even though no network request occurred.
    fn start(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                ACTIVE => return true,
                FINISHED => return false,
                DORMANT => {
                    if self
                        .state
                        .compare_exchange(DORMANT, ACTIVE, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        super::show_progress_notification(&self.title, &self.initial_message, 0.0);
                        return true;
                    }
                }
                _ => unreachable!("invalid download badge state"),
            }
        }
    }

    pub fn finish(&self) {
        if self.state.swap(FINISHED, Ordering::AcqRel) == ACTIVE {
            super::hide_progress_notification_for(&self.title);
        }
    }
}

impl Drop for DownloadProgressBadge {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_only_badge_stays_silent() {
        let badge = DownloadProgressBadge::with_messages("component", "preparing", "downloading");
        assert_eq!(badge.state.load(Ordering::Acquire), DORMANT);
        badge.finish();
        assert_eq!(badge.state.load(Ordering::Acquire), FINISHED);
        assert!(!badge.start());
    }
}
