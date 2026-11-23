use super::preferences::Preferences;
use crate::admin_startup::{self, Issue, Mode, Reconciled, provider::CurrentUserStartup};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub(super) trait Backend {
    fn normal_enabled(&mut self, authorized: &str) -> Result<bool>;
    fn reconcile(&mut self, authorized: &str) -> (Reconciled, Option<Issue>);
    fn previous_missing(&mut self, authorized: &str) -> bool;
    fn change(&mut self, mode: Mode, authorized: &str) -> Result<()>;
}

pub(super) struct Native {
    pub path: PathBuf,
}

impl Backend for Native {
    fn normal_enabled(&mut self, authorized: &str) -> Result<bool> {
        CurrentUserStartup::new(&self.path, authorized).is_enabled()
    }

    fn reconcile(&mut self, authorized: &str) -> (Reconciled, Option<Issue>) {
        let state = admin_startup::reconcile(authorized);
        (state, admin_startup::last_issue())
    }

    fn previous_missing(&mut self, authorized: &str) -> bool {
        admin_startup::previous_copy_is_missing(authorized)
    }

    fn change(&mut self, mode: Mode, authorized: &str) -> Result<()> {
        let launcher = CurrentUserStartup::new(&self.path, authorized);
        if admin_startup::set_mode(&launcher, mode, authorized) {
            Ok(())
        } else {
            Err(admin_startup::last_issue()
                .unwrap_or(Issue::OperationFailed)
                .into())
        }
    }
}

pub(super) struct Outcome {
    pub preferences: Preferences,
    pub running: bool,
    pub issue: Option<Issue>,
    pub success: bool,
}

pub(super) fn failed(preferences: Preferences, running: bool, issue: Issue) -> Outcome {
    Outcome {
        preferences,
        running,
        issue: Some(issue),
        success: false,
    }
}

pub(super) fn change(
    backend: &mut impl Backend,
    mut preferences: Preferences,
    running: bool,
    mode: Mode,
    path: &Path,
) -> Outcome {
    if let Err(error) = backend.change(mode, &preferences.authorized) {
        let issue = error
            .downcast_ref::<Issue>()
            .copied()
            .unwrap_or(Issue::OperationFailed);
        return failed(preferences, running, issue);
    }
    preferences.set_mode(mode, path);
    Outcome {
        preferences,
        running: !matches!(mode, Mode::Off),
        issue: None,
        success: true,
    }
}

pub(super) fn reconcile(
    backend: &mut impl Backend,
    mut preferences: Preferences,
    path: &Path,
    administrator: bool,
) -> Outcome {
    let registry = backend.normal_enabled(&preferences.authorized);
    let normal = registry.as_ref().copied().unwrap_or(false);
    if normal && !preferences.normal && !preferences.elevated {
        preferences.normal = true;
        if preferences.authorized.is_empty() {
            preferences.adopt(path);
        }
    }
    let (state, mut issue) = backend.reconcile(&preferences.authorized);
    if registry.is_err() {
        issue = Some(Issue::OperationFailed);
    }
    let elevated = state == Reconciled::Enabled;
    if elevated && !preferences.elevated {
        preferences.elevated = true;
        if preferences.authorized.is_empty() {
            preferences.adopt(path);
        }
    }
    let authorized = preferences.authorized.is_empty()
        || path
            .to_str()
            .is_some_and(|path| path == preferences.authorized)
        || backend.previous_missing(&preferences.authorized);
    let mut running = if preferences.elevated {
        elevated
    } else {
        normal && preferences.normal
    };
    if authorized {
        let mode = if preferences.elevated {
            ((!elevated || normal) && (administrator || elevated) && state != Reconciled::Unknown)
                .then_some(Mode::Elevated)
        } else {
            preferences.normal.then_some(Mode::Normal)
        };
        if let Some(mode) = mode {
            match backend.change(mode, &preferences.authorized) {
                Ok(()) => {
                    preferences.adopt(path);
                    running = true;
                    issue = None;
                }
                Err(error) => {
                    issue = Some(
                        error
                            .downcast_ref::<Issue>()
                            .copied()
                            .unwrap_or(Issue::OperationFailed),
                    );
                    if matches!(mode, Mode::Normal) {
                        running = false;
                    }
                }
            }
        }
    }
    Outcome {
        preferences,
        running,
        issue,
        success: true,
    }
}
