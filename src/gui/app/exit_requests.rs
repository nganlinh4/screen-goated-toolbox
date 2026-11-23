use std::sync::Mutex;

enum Intent {
    Quit,
    Restart,
    Update(crate::updater::StagedUpdate),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Committed {
    Automatic,
    Explicit,
}

struct Pending<T> {
    intent: Option<T>,
    conflict_shown: bool,
    launch_failed: bool,
    active: bool,
    committed: Option<Committed>,
}

impl<T> Pending<T> {
    const fn new() -> Self {
        Self {
            intent: None,
            conflict_shown: false,
            launch_failed: false,
            active: false,
            committed: None,
        }
    }

    fn request(&mut self, intent: T) -> bool {
        if self.committed.is_some() {
            return false;
        }
        self.intent = Some(intent);
        self.launch_failed = false;
        self.conflict_shown = false;
        true
    }

    fn defer_busy(&mut self, intent: T) {
        if self.intent.is_none() {
            self.intent = Some(intent);
        }
    }

    fn next(&mut self, conflict: bool) -> (Option<T>, bool) {
        if self.launch_failed {
            return (None, false);
        }
        if conflict && self.intent.is_some() {
            let show = !std::mem::replace(&mut self.conflict_shown, true);
            return (None, show);
        }
        self.conflict_shown = false;
        let intent = self.intent.take();
        self.active = intent.is_some();
        (intent, false)
    }

    fn launch_failure(&mut self, intent: T) -> bool {
        self.active = false;
        if self.intent.is_some() || self.committed.is_some() {
            return false;
        }
        self.intent = Some(intent);
        self.launch_failed = true;
        true
    }

    fn busy(&self) -> bool {
        self.active || self.intent.is_some() || self.committed.is_some()
    }

    fn reserve_automatic<R>(
        &mut self,
        launch: impl FnOnce() -> std::io::Result<R>,
    ) -> std::io::Result<R> {
        if self.busy() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "an explicit exit request is pending",
            ));
        }
        let result = launch()?;
        self.committed = Some(Committed::Automatic);
        Ok(result)
    }

    fn reserve_explicit<R>(
        &mut self,
        commit: impl FnOnce() -> std::io::Result<R>,
    ) -> std::io::Result<R> {
        if self.intent.is_some() || self.committed.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "another exit request is pending or committed",
            ));
        }
        let result = commit()?;
        self.committed = Some(Committed::Explicit);
        Ok(result)
    }

    fn resolve_failure(&mut self, retry: bool) {
        if self.launch_failed {
            self.launch_failed = false;
            if !retry {
                self.intent = None;
            }
        }
    }
}

static PENDING: Mutex<Pending<Intent>> = Mutex::new(Pending::new());

fn request(intent: Intent) {
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    let accepted = pending.request(intent);
    let committed = pending.committed;
    drop(pending);
    if !accepted {
        let badge = crate::overlay::auto_copy_badge::locale_text();
        let (title, message) = match committed {
            Some(_) => (
                badge.restart_in_progress_title,
                badge.restart_in_progress_body,
            ),
            None => return,
        };
        crate::overlay::auto_copy_badge::show_detailed_notification(
            title,
            message,
            crate::overlay::auto_copy_badge::NotificationType::Info,
        );
        return;
    }
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.request_repaint();
    }
}

pub fn exit_app() {
    request(Intent::Quit);
}
pub fn restart_app() {
    request(Intent::Restart);
}
pub(crate) fn request_update_restart(staged: crate::updater::StagedUpdate) {
    request(Intent::Update(staged));
}

pub(crate) fn has_pending_exit_request() -> bool {
    PENDING.lock().map_or(true, |pending| pending.busy())
}

/// Linearize automatic helper launch with admission of explicit exit requests.
pub(crate) struct AutomaticRestartReservation {
    _activation: crate::app_activation::admission::AdmissionGuard,
}

impl Drop for AutomaticRestartReservation {
    fn drop(&mut self) {
        if let Ok(mut pending) = PENDING.lock()
            && pending.committed == Some(Committed::Automatic)
        {
            pending.committed = None;
        }
    }
}

pub(crate) fn reserve_automatic_restart(
    launch: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<AutomaticRestartReservation> {
    let activation = crate::app_activation::reserve_restart_admission()?;
    let mut pending = PENDING
        .lock()
        .map_err(|_| std::io::Error::other("exit request lock poisoned"))?;
    pending.reserve_automatic(launch)?;
    Ok(AutomaticRestartReservation {
        _activation: activation,
    })
}

pub(super) fn reserve_explicit_exit<R>(
    commit: impl FnOnce() -> std::io::Result<R>,
) -> std::io::Result<R> {
    let mut pending = PENDING
        .lock()
        .map_err(|_| std::io::Error::other("exit request lock poisoned"))?;
    pending.reserve_explicit(commit)
}

pub(super) fn dispatch(conflict: bool, ctx: &eframe::egui::Context) {
    let (intent, show_conflict) = match PENDING.lock() {
        Ok(mut pending) => pending.next(conflict),
        Err(_) => return,
    };
    if show_conflict {
        super::signal_restore_window();
    }
    let Some(intent) = intent else { return };
    let prepared = match super::exit_preparation::try_prepare_to_exit() {
        Ok(prepared) => prepared,
        Err(super::exit_preparation::PrepareDenied::Retry) => {
            if let Ok(mut pending) = PENDING.lock() {
                pending.active = false;
                pending.defer_busy(intent);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        Err(super::exit_preparation::PrepareDenied::Failed) => {
            let blocked = PENDING
                .lock()
                .is_ok_and(|mut pending| pending.launch_failure(intent));
            if blocked {
                super::signal_restore_window();
            }
            ctx.request_repaint();
            return;
        }
    };
    if PENDING.lock().is_ok_and(|pending| pending.intent.is_some()) {
        drop(prepared);
        if let Ok(mut pending) = PENDING.lock() {
            pending.active = false;
        }
        ctx.request_repaint();
        return;
    }
    let result = match &intent {
        Intent::Quit => reserve_explicit_exit(|| -> std::io::Result<()> {
            super::exit_preparation::exit_prepared(prepared)
        })
        .map_err(anyhow::Error::from),
        Intent::Restart => super::exit_preparation::perform_restart(prepared),
        Intent::Update(staged) => super::exit_update::perform_update(staged, prepared),
    };
    if let Err(error) = result {
        crate::log_info!("[Shutdown] could not launch restart: {error:#}");
        if PENDING
            .lock()
            .is_ok_and(|mut pending| pending.launch_failure(intent))
        {
            super::signal_restore_window();
        }
    } else if let Ok(mut pending) = PENDING.lock() {
        pending.active = false;
    }
    if PENDING.lock().is_ok_and(|pending| pending.intent.is_some()) {
        ctx.request_repaint();
    }
}

pub(super) fn render_failure(ui: &mut eframe::egui::Ui, language: &str) {
    if !PENDING.lock().is_ok_and(|pending| pending.launch_failed) {
        return;
    }
    let text = crate::gui::locale::LocaleText::get(language);
    let theme = crate::gui::theme::AppTheme::from_ui(ui);
    let mut choice = None;
    crate::gui::widgets::material_modal(
        ui.ctx(),
        &theme,
        eframe::egui::Id::new("restart_launch_failed"),
        |ui| {
            ui.set_width(520.0);
            ui.heading(text.shell.restart_launch_failed_title);
            ui.add_space(8.0);
            ui.label(text.shell.restart_launch_failed_body);
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if crate::gui::widgets::filled_button(
                    ui,
                    text.shell.restart_launch_retry,
                    theme.accent_fill(),
                    theme.on_accent(),
                    16,
                )
                .clicked()
                {
                    choice = Some(true);
                }
                if crate::gui::widgets::filled_button(
                    ui,
                    text.shell.restart_launch_cancel,
                    theme.neutral_fill(),
                    theme.on_surface(),
                    16,
                )
                .clicked()
                {
                    choice = Some(false);
                }
            });
        },
    );
    if let Some(retry) = choice
        && let Ok(mut pending) = PENDING.lock()
    {
        pending.resolve_failure(retry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_user_exit_intents_wait_for_conflict_resolution() {
        for intent in ["quit", "restart", "update"] {
            let mut pending = Pending::new();
            pending.request(intent);
            assert_eq!(pending.next(true), (None, true));
            assert_eq!(pending.next(true), (None, false));
            assert_eq!(pending.next(false), (Some(intent), false));
        }
    }

    #[test]
    fn newer_request_supersedes_busy_preparation() {
        let mut pending = Pending::new();
        pending.request("update");
        let first = pending.next(false).0.unwrap();
        pending.request("quit");
        pending.defer_busy(first);
        assert_eq!(pending.next(false).0, Some("quit"));
        assert_eq!(pending.next(false).0, None);
    }

    #[test]
    fn latest_request_wins_while_conflict_is_open() {
        for (first, latest) in [("quit", "update"), ("update", "quit")] {
            let mut pending = Pending::new();
            pending.request(first);
            assert_eq!(pending.next(true), (None, true));
            pending.request(latest);
            assert_eq!(pending.next(true), (None, true));
            assert_eq!(pending.next(false).0, Some(latest));
        }
    }

    #[test]
    fn queued_and_in_flight_explicit_requests_block_automatic_restart() {
        let mut pending = Pending::new();
        assert!(!pending.busy());
        pending.request("quit");
        assert!(pending.busy());
        let intent = pending.next(false).0.unwrap();
        assert!(pending.busy());
        pending.launch_failure(intent);
        assert!(pending.busy());
        pending.resolve_failure(false);
        assert!(!pending.busy());
    }

    #[test]
    fn admitted_explicit_request_prevents_automatic_helper_spawn() {
        use std::cell::Cell;
        let spawned = Cell::new(false);
        let mut pending = Pending::new();
        pending.request("update");
        let result = pending.reserve_automatic(|| {
            spawned.set(true);
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
        assert!(!spawned.get());
        assert_eq!(pending.next(false).0, Some("update"));
        assert!(pending.reserve_automatic(|| Ok(())).is_err());
    }

    #[test]
    fn automatic_commit_rejects_later_requests_until_reservation_released() {
        let mut pending = Pending::new();
        pending.reserve_automatic(|| Ok(())).unwrap();
        assert!(pending.busy());
        assert!(!pending.request("quit"));
        assert!(!pending.request("update"));
        assert!(pending.intent.is_none());
        pending.committed = None;
        assert!(pending.request("quit"));
        assert_eq!(pending.next(false).0, Some("quit"));
    }

    #[test]
    fn newer_explicit_request_prevents_older_commit() {
        use std::cell::Cell;
        let mut pending = Pending::new();
        pending.request("quit");
        assert_eq!(pending.next(false).0, Some("quit"));
        pending.request("update");
        let committed = Cell::new(false);
        let result = pending.reserve_explicit(|| {
            committed.set(true);
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
        assert!(!committed.get());
        assert_eq!(pending.next(false).0, Some("update"));
    }

    #[test]
    fn explicit_helper_commit_rejects_requests_after_spawn() {
        for first in ["restart", "update"] {
            let mut pending = Pending::new();
            assert!(pending.request(first));
            assert_eq!(pending.next(false).0, Some(first));
            let spawned = std::cell::Cell::new(false);
            pending
                .reserve_explicit(|| {
                    spawned.set(true);
                    Ok(())
                })
                .unwrap();
            assert!(spawned.get());
            assert!(pending.committed == Some(Committed::Explicit));
            assert!(pending.reserve_explicit(|| Ok(())).is_err());
            for later in ["quit", "restart", "update"] {
                assert!(!pending.request(later));
            }
            assert!(pending.intent.is_none());
        }
    }

    #[test]
    fn failed_explicit_spawn_does_not_commit_or_block_retry() {
        let mut pending = Pending::new();
        assert!(pending.request("restart"));
        assert_eq!(pending.next(false).0, Some("restart"));
        assert!(
            pending
                .reserve_explicit(|| Err::<(), _>(std::io::Error::other("failed")))
                .is_err()
        );
        assert!(pending.committed.is_none());
        assert!(pending.launch_failure("restart"));
        pending.resolve_failure(true);
        assert_eq!(pending.next(false).0, Some("restart"));
    }

    #[test]
    fn launch_failure_keeps_intent_until_retry_or_cancel() {
        for retry in [true, false] {
            let mut pending = Pending::new();
            pending.launch_failure("update");
            assert_eq!(pending.next(false).0, None);
            pending.resolve_failure(retry);
            assert_eq!(pending.next(false).0, retry.then_some("update"));
        }
    }
}
