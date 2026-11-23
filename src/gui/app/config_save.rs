//! Coalesced persistence of settings-window edits.
//!
//! Edits reach the shared in-memory config at once, so overlays and hotkeys
//! follow a slider while it moves. The fsynced disk write waits until edits
//! have been idle for `SAVE_IDLE_DELAY`, and is forced when the window closes,
//! loses focus, or an exit is requested.

use std::time::{Duration, Instant};

use super::config_merge::{ConfigConflict, merge_config, merge_settings_edits};
use crate::config::Config;
use eframe::egui;

pub(super) const SAVE_IDLE_DELAY: Duration = Duration::from_millis(500);

pub(super) struct PendingSave {
    /// The settings the window's unsaved edits were made against.
    base: Config,
    last_edit: Instant,
}

#[derive(Default)]
pub(super) struct ConfigSaveQueue {
    pending: Option<PendingSave>,
}

impl ConfigSaveQueue {
    /// Keep other writers' values out of the settings window's rollback batch.
    pub(super) fn absorb_shared_changes(
        &mut self,
        base: &Config,
        current: &Config,
    ) -> anyhow::Result<()> {
        if let Some(pending) = &mut self.pending {
            pending.base = merge_settings_edits(base, current, &pending.base)?;
        }
        Ok(())
    }
    /// Notes an edit committed in memory. `base` is the window's baseline
    /// before that edit; only the first edit of a batch keeps it.
    pub(super) fn record_edit(&mut self, base: &Config, now: Instant) {
        match &mut self.pending {
            Some(pending) => pending.last_edit = now,
            None => {
                self.pending = Some(PendingSave {
                    base: base.clone(),
                    last_edit: now,
                })
            }
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// How long until the pending write is due: `None` when nothing waits,
    /// zero when it should be written now.
    pub(super) fn wait(&self, now: Instant, force: bool) -> Option<Duration> {
        let pending = self.pending.as_ref()?;
        if force {
            return Some(Duration::ZERO);
        }
        Some(SAVE_IDLE_DELAY.saturating_sub(now.saturating_duration_since(pending.last_edit)))
    }

    fn take(&mut self) -> Option<PendingSave> {
        self.pending.take()
    }
}

/// Writes the shared config. When the write fails, the window's unsaved edits
/// are taken back out of `current` (other writers' changes stay), and the
/// returned conflict holds them for Retry or Discard.
pub(super) fn persist(
    pending: PendingSave,
    committed: &Config,
    draft: &Config,
    current: &mut Config,
    save: impl FnOnce(&Config) -> std::io::Result<()>,
) -> Result<(), Box<ConfigConflict>> {
    let Err(error) = save(current) else {
        return Ok(());
    };
    crate::log_info!("[config] failed to save settings edits: {error}");
    match merge_config(committed, &pending.base, current) {
        Ok(reverted) => *current = reverted,
        // Another writer changed the same values since; keep memory as it is
        // rather than drop their change. Nothing is lost either way.
        Err(error) => crate::log_info!("[config] could not withdraw unsaved edits: {error}"),
    }
    Err(Box::new(ConfigConflict {
        base: pending.base,
        edited: draft.clone(),
    }))
}

impl super::SettingsApp {
    /// Writes coalesced edits once they are idle, or at once when the window
    /// closes, loses focus, or an exit waits for the write.
    pub(super) fn flush_config_save_when_due(&mut self, ctx: &egui::Context) {
        let force = ctx.input(|input| input.viewport().close_requested() || !input.focused)
            || super::exit_requests::has_pending_exit_request();
        match self.config_save.wait(Instant::now(), force) {
            None => {}
            Some(wait) if !wait.is_zero() => ctx.request_repaint_after(wait),
            Some(_) => self.flush_config_save_now(),
        }
    }

    pub(super) fn flush_config_save_now(&mut self) {
        let Some(pending) = self.config_save.take() else {
            return;
        };
        let mut state = self.app_state_ref.lock().unwrap();
        let result = persist(
            pending,
            &self.config_baseline,
            &self.config,
            &mut state.config,
            crate::config::try_save_config,
        );
        if let Err(conflict) = result {
            self.config_baseline = state.config.clone();
            drop(state);
            self.config_conflict = Some(*conflict);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited(base: &Config) -> Config {
        let mut edited = base.clone();
        edited.max_history_items += 1;
        edited
    }

    #[test]
    fn failed_write_after_refresh_preserves_shared_changes_inside_the_save_batch() {
        let base = Config::default();
        let first = edited(&base);
        let mut queue = ConfigSaveQueue::default();
        queue.record_edit(&base, Instant::now());
        let mut shared = first.clone();
        shared.tts_playground.draft_text = "saved in another view".into();
        queue.absorb_shared_changes(&first, &shared).unwrap();
        let mut draft = shared.clone();
        draft.max_history_items += 1;
        let mut current = draft.clone();
        let failure = persist(queue.take().unwrap(), &draft, &draft, &mut current, |_| {
            Err(std::io::Error::other("disk unavailable"))
        })
        .unwrap_err();
        assert_eq!(current.max_history_items, base.max_history_items);
        assert_eq!(
            current.tts_playground.draft_text,
            shared.tts_playground.draft_text
        );
        let retry = merge_settings_edits(&failure.base, &failure.edited, &current).unwrap();
        assert_eq!(retry.max_history_items, draft.max_history_items);
        assert_eq!(
            retry.tts_playground.draft_text,
            shared.tts_playground.draft_text
        );
    }

    #[test]
    fn edits_within_the_idle_delay_coalesce_into_one_write() {
        let base = Config::default();
        let start = Instant::now();
        let mut queue = ConfigSaveQueue::default();
        assert_eq!(queue.wait(start, false), None);
        for frame in 0..30u64 {
            let now = start + Duration::from_millis(frame * 16);
            queue.record_edit(&base, now);
            assert!(queue.wait(now, false).is_some_and(|wait| !wait.is_zero()));
        }
        let last_edit = start + Duration::from_millis(29 * 16);
        let due = last_edit + SAVE_IDLE_DELAY;
        assert!(
            !queue
                .wait(due - Duration::from_millis(1), false)
                .unwrap()
                .is_zero()
        );
        assert!(queue.wait(due, false).unwrap().is_zero());

        let draft = edited(&base);
        let mut current = draft.clone();
        let mut writes = 0;
        let saved = persist(
            queue.take().unwrap(),
            &draft,
            &draft,
            &mut current,
            |saved| {
                writes += 1;
                assert_eq!(saved.max_history_items, draft.max_history_items);
                Ok(())
            },
        );
        assert!(saved.is_ok());
        assert_eq!(writes, 1);
        assert!(!queue.is_pending());
    }

    #[test]
    fn close_focus_loss_or_exit_writes_at_once() {
        let base = Config::default();
        let now = Instant::now();
        let mut queue = ConfigSaveQueue::default();
        queue.record_edit(&base, now);
        assert_eq!(queue.wait(now, true), Some(Duration::ZERO));
    }

    #[test]
    fn the_first_edit_of_a_batch_keeps_the_saved_base() {
        let base = Config::default();
        let now = Instant::now();
        let mut queue = ConfigSaveQueue::default();
        queue.record_edit(&base, now);
        queue.record_edit(&edited(&base), now);
        assert_eq!(
            queue.take().unwrap().base.max_history_items,
            base.max_history_items
        );
    }

    #[test]
    fn failed_write_withdraws_unsaved_edits_and_keeps_the_draft_for_retry() {
        let base = Config::default();
        let draft = edited(&base);
        let mut current = draft.clone();
        current.tts_playground.draft_text = "saved elsewhere".into();
        let mut queue = ConfigSaveQueue::default();
        queue.record_edit(&base, Instant::now());

        let conflict = persist(queue.take().unwrap(), &draft, &draft, &mut current, |_| {
            Err(std::io::Error::other("disk unavailable"))
        })
        .unwrap_err();
        assert_eq!(conflict.base.max_history_items, base.max_history_items);
        assert_eq!(conflict.edited.max_history_items, draft.max_history_items);
        assert_eq!(current.max_history_items, base.max_history_items);
        assert_eq!(current.tts_playground.draft_text, "saved elsewhere");

        // Retry: the draft merges back onto the latest settings and is written.
        let retried = merge_config(&conflict.base, &conflict.edited, &current).unwrap();
        assert_eq!(retried.max_history_items, draft.max_history_items);
        assert_eq!(retried.tts_playground.draft_text, "saved elsewhere");
    }
}
