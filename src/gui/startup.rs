//! Serialized startup provider work with UI-owned completion and preferences.
mod preferences;
mod reconcile;

use crate::admin_startup::{Issue, Mode};
use crate::config::{Config, RestoreDefaultsSelection};
use eframe::egui;
use preferences::Preferences;
use reconcile::Outcome;
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};

static NATIVE_LANE: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
enum Kind {
    Reconcile(bool),
    Change(Mode),
    Restore(RestoreDefaultsSelection),
}

struct Pending {
    expected: Preferences,
    kind: Kind,
    receiver: mpsc::Receiver<Outcome>,
    permit: Option<std::sync::Arc<crate::gui::app::StartupPermit<'static>>>,
}

pub(crate) struct Startup {
    path: Option<PathBuf>,
    system_disabled: bool,
    pending: Option<Pending>,
    issue: Option<Issue>,
}

pub(crate) struct Completion {
    pub changed: bool,
    pub restore: Option<RestoreDefaultsSelection>,
    pub restore_failed: bool,
    _permit: Option<std::sync::Arc<crate::gui::app::StartupPermit<'static>>>,
}

impl Startup {
    pub fn disabled() -> Self {
        Self {
            path: None,
            system_disabled: true,
            pending: None,
            issue: None,
        }
    }

    pub fn new(config: &Config, administrator: bool, ctx: &egui::Context) -> Self {
        if crate::paths::runtime_state_root().is_some() {
            return Self::disabled();
        }
        let mut owner = Self::with_path(std::env::current_exe());
        if owner.available() {
            owner.start(
                config,
                config.run_at_startup || config.run_as_admin_on_startup,
                Kind::Reconcile(administrator),
                ctx,
            );
        }
        owner
    }

    fn with_path(path: std::io::Result<PathBuf>) -> Self {
        let mut owner = Self::disabled();
        owner.system_disabled = false;
        match path {
            Ok(path) => {
                owner.path = Some(path);
            }
            Err(_) => owner.issue = Some(Issue::OperationFailed),
        }
        owner
    }

    pub fn available(&self) -> bool {
        self.path.is_some()
    }
    pub fn system_disabled(&self) -> bool {
        self.system_disabled
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn issue(&self) -> Option<Issue> {
        self.issue
    }

    pub fn request(
        &mut self,
        config: &Config,
        running: bool,
        mode: Mode,
        ctx: &egui::Context,
    ) -> bool {
        self.start(config, running, Kind::Change(mode), ctx)
    }

    pub fn request_restore(
        &mut self,
        config: &Config,
        running: bool,
        selection: RestoreDefaultsSelection,
        ctx: &egui::Context,
    ) -> bool {
        self.start(config, running, Kind::Restore(selection), ctx)
    }

    fn start(&mut self, config: &Config, running: bool, kind: Kind, ctx: &egui::Context) -> bool {
        if self.busy() {
            return false;
        }
        let Some(path) = self.path.clone() else {
            return false;
        };
        let Some(permit) = crate::gui::app::begin_startup_operation() else {
            return false;
        };
        let worker_permit = permit.clone();
        let expected = Preferences::capture(config);
        let preferences = expected.clone();
        let ctx = ctx.clone();
        self.start_with(
            expected,
            kind,
            Some(permit),
            move || {
                let _permit = worker_permit;
                serialized(&NATIVE_LANE, preferences.clone(), running, || {
                    let mut backend = reconcile::Native { path: path.clone() };
                    match kind {
                        Kind::Reconcile(administrator) => {
                            reconcile::reconcile(&mut backend, preferences, &path, administrator)
                        }
                        Kind::Change(mode) => {
                            reconcile::change(&mut backend, preferences, running, mode, &path)
                        }
                        Kind::Restore(_) => {
                            reconcile::change(&mut backend, preferences, running, Mode::Off, &path)
                        }
                    }
                })
            },
            move || ctx.request_repaint(),
        )
    }

    fn start_with(
        &mut self,
        expected: Preferences,
        kind: Kind,
        permit: Option<std::sync::Arc<crate::gui::app::StartupPermit<'static>>>,
        work: impl FnOnce() -> Outcome + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) -> bool {
        if self.busy() {
            return false;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let failure = expected.clone();
        let spawn = std::thread::Builder::new()
            .name("startup-provider".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                    .unwrap_or_else(|_| reconcile::failed(failure, false, Issue::UncertainState));
                let _ = sender.send(outcome);
                wake();
            });
        if let Err(error) = spawn {
            crate::log_info!("[Startup] Could not start provider worker: {error}");
            self.issue = Some(Issue::OperationFailed);
            return false;
        }
        self.pending = Some(Pending {
            expected,
            kind,
            receiver,
            permit,
        });
        true
    }

    pub fn poll(&mut self, config: &mut Config, running: &mut bool) -> Option<Completion> {
        let pending = self.pending.as_ref()?;
        let outcome = match pending.receiver.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                reconcile::failed(pending.expected.clone(), *running, Issue::UncertainState)
            }
        };
        let pending = self.pending.take().unwrap();
        if Preferences::capture(config) != pending.expected {
            self.issue = Some(Issue::UncertainState);
            return Some(Completion {
                changed: false,
                restore: None,
                restore_failed: matches!(pending.kind, Kind::Restore(_)),
                _permit: pending.permit,
            });
        }
        self.issue = outcome.issue;
        if let Kind::Restore(selection) = pending.kind {
            let verified_off = outcome.success
                && outcome.issue.is_none()
                && !outcome.running
                && !outcome.preferences.normal
                && !outcome.preferences.elevated;
            if verified_off {
                *running = false;
            } else if outcome.success {
                self.issue = Some(Issue::UncertainState);
            }
            return Some(Completion {
                changed: false,
                restore: verified_off.then_some(selection),
                restore_failed: !verified_off,
                _permit: pending.permit,
            });
        }
        let changed = outcome.preferences != pending.expected;
        if outcome.success {
            outcome.preferences.write(config);
            *running = outcome.running;
        }
        Some(Completion {
            changed: changed && outcome.success,
            restore: None,
            restore_failed: false,
            _permit: pending.permit,
        })
    }
}

fn serialized(
    lane: &Mutex<()>,
    preferences: Preferences,
    running: bool,
    work: impl FnOnce() -> Outcome,
) -> Outcome {
    let Ok(_guard) = lane.lock() else {
        return reconcile::failed(preferences, running, Issue::UncertainState);
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| reconcile::failed(preferences, running, Issue::UncertainState))
}

#[cfg(test)]
mod tests;
