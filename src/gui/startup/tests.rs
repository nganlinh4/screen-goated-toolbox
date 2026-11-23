use super::*;
use crate::admin_startup::Reconciled;
use anyhow::Result;
use reconcile::Backend;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

struct Fake {
    normal: bool,
    registry_error: bool,
    task: Reconciled,
    issue: Option<Issue>,
    missing: bool,
    change_issue: Option<Issue>,
    changes: Vec<(Mode, String)>,
    observations: Vec<String>,
}

impl Fake {
    fn new() -> Self {
        Self {
            normal: false,
            registry_error: false,
            task: Reconciled::Missing,
            issue: None,
            missing: false,
            change_issue: None,
            changes: vec![],
            observations: vec![],
        }
    }
}

impl Backend for Fake {
    fn normal_enabled(&mut self, authorized: &str) -> Result<bool> {
        self.observations.push(authorized.to_owned());
        anyhow::ensure!(!self.registry_error, "registry fixture failure");
        Ok(self.normal)
    }
    fn reconcile(&mut self, authorized: &str) -> (Reconciled, Option<Issue>) {
        self.observations.push(authorized.to_owned());
        (self.task, self.issue)
    }
    fn previous_missing(&mut self, authorized: &str) -> bool {
        self.observations.push(authorized.to_owned());
        self.missing
    }
    fn change(&mut self, mode: Mode, authorized: &str) -> Result<()> {
        self.changes.push((mode, authorized.to_owned()));
        if let Some(issue) = self.change_issue {
            Err(issue.into())
        } else {
            Ok(())
        }
    }
}

fn preferences() -> Preferences {
    Preferences::capture(&Config::default())
}
fn outcome(preferences: Preferences) -> Outcome {
    Outcome {
        preferences,
        running: true,
        issue: None,
        success: true,
    }
}
fn forbidden() -> Outcome {
    panic!("an overlapping operation was started")
}
fn no_wake() {}

#[test]
fn successful_modes_commit_only_startup_preferences_after_provider_success() {
    for mode in [Mode::Normal, Mode::Elevated, Mode::Off] {
        let mut config = Config {
            start_in_tray: true,
            ui_language: "ko".into(),
            authorized_startup_path: "prior".into(),
            ..Config::default()
        };
        let mut backend = Fake::new();
        let after = reconcile::change(
            &mut backend,
            Preferences::capture(&config),
            true,
            mode,
            Path::new("current.exe"),
        );
        assert!(after.success && after.issue.is_none());
        assert_eq!(backend.changes, vec![(mode, "prior".into())]);
        after.preferences.write(&mut config);
        assert_eq!(config.run_at_startup, matches!(mode, Mode::Normal));
        assert_eq!(
            config.run_as_admin_on_startup,
            matches!(mode, Mode::Elevated)
        );
        assert_eq!(config.start_in_tray, !matches!(mode, Mode::Off));
        assert_eq!(after.running, !matches!(mode, Mode::Off));
        assert_eq!(config.authorized_startup_path, "current.exe");
        assert_eq!(config.ui_language, "ko");
    }
}

#[test]
fn rejected_and_uncertain_changes_preserve_preferences_and_running_state() {
    for mode in [Mode::Normal, Mode::Elevated, Mode::Off] {
        for issue in [Issue::ProtectedLocationRequired, Issue::UncertainState] {
            for running in [false, true] {
                let before = Preferences {
                    normal: true,
                    elevated: false,
                    tray: true,
                    authorized: "prior".into(),
                };
                let mut backend = Fake::new();
                backend.change_issue = Some(issue);
                let after = reconcile::change(
                    &mut backend,
                    before.clone(),
                    running,
                    mode,
                    Path::new("current.exe"),
                );
                assert!(!after.success);
                assert_eq!(after.preferences, before);
                assert_eq!(after.running, running);
                assert_eq!(after.issue, Some(issue));
            }
        }
    }
}

#[test]
fn reconciliation_migrates_observed_normal_registration_before_coordinated_repair() {
    let mut backend = Fake::new();
    backend.normal = true;
    let after = reconcile::reconcile(&mut backend, preferences(), Path::new("current.exe"), false);
    assert!(after.preferences.normal && !after.preferences.elevated && after.running);
    assert_eq!(after.preferences.authorized, "current.exe");
    assert_eq!(backend.observations, ["", "current.exe"]);
    assert_eq!(backend.changes, vec![(Mode::Normal, "current.exe".into())]);
}

#[test]
fn reconciliation_selects_elevation_when_both_providers_are_observed() {
    let mut backend = Fake::new();
    backend.normal = true;
    backend.task = Reconciled::Enabled;
    let after = reconcile::reconcile(&mut backend, preferences(), Path::new("current.exe"), false);
    assert!(after.running && after.preferences.elevated);
    assert_eq!(
        backend.changes,
        vec![(Mode::Elevated, "current.exe".into())]
    );
}

#[test]
fn unknown_or_retired_elevation_never_triggers_unprivileged_automatic_repair() {
    for (task, issue) in [
        (Reconciled::Unknown, Issue::UncertainState),
        (Reconciled::Retired, Issue::ProtectedLocationRequired),
    ] {
        let mut backend = Fake::new();
        backend.task = task;
        backend.issue = Some(issue);
        let mut before = preferences();
        before.elevated = true;
        let after = reconcile::reconcile(
            &mut backend,
            before.clone(),
            Path::new("current.exe"),
            false,
        );
        assert!(backend.changes.is_empty());
        assert_eq!(after.preferences, before);
        assert!(!after.running);
        assert_eq!(after.issue, Some(issue));
    }
    let mut backend = Fake::new();
    backend.task = Reconciled::Unknown;
    let mut before = preferences();
    before.elevated = true;
    reconcile::reconcile(&mut backend, before, Path::new("current.exe"), true);
    assert!(backend.changes.is_empty());
}

#[test]
fn takeover_requires_confirmed_previous_copy_absence_and_advances_only_after_success() {
    for missing in [false, true] {
        for success in [false, true] {
            let mut backend = Fake::new();
            backend.missing = missing;
            backend.change_issue = (!success).then_some(Issue::UncertainState);
            let before = Preferences {
                normal: true,
                elevated: false,
                tray: true,
                authorized: "other.exe".into(),
            };
            let after = reconcile::reconcile(
                &mut backend,
                before.clone(),
                Path::new("current.exe"),
                false,
            );
            assert_eq!(backend.changes.len(), usize::from(missing));
            assert_eq!(after.running, missing && success);
            assert_eq!(
                after.preferences.authorized,
                if missing && success {
                    "current.exe"
                } else {
                    "other.exe"
                }
            );
        }
    }
}

#[test]
fn registry_error_is_retained_when_existing_elevation_needs_no_transition() {
    let mut backend = Fake::new();
    backend.registry_error = true;
    backend.task = Reconciled::Enabled;
    let mut before = preferences();
    before.elevated = true;
    let after = reconcile::reconcile(&mut backend, before, Path::new("current.exe"), true);
    assert!(after.running && backend.changes.is_empty());
    assert_eq!(after.issue, Some(Issue::OperationFailed));
}

#[test]
fn pending_work_is_nonblocking_single_and_preserves_unrelated_frame_edits() -> Result<()> {
    let mut owner = Startup::disabled();
    let mut config = Config::default();
    let mut running = false;
    let expected = Preferences::capture(&config);
    let mut desired = expected.clone();
    desired.set_mode(Mode::Normal, Path::new("current.exe"));
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let (woke, wake) = mpsc::sync_channel(1);
    assert!(owner.start_with(
        expected.clone(),
        Kind::Change(Mode::Normal),
        None,
        move || {
            entered.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            outcome(desired)
        },
        move || woke.send(()).unwrap()
    ));
    entry.recv_timeout(Duration::from_secs(5))?;
    config.ui_language = "ko".into();
    for _ in 0..100 {
        assert!(owner.busy());
        assert!(owner.poll(&mut config, &mut running).is_none());
        assert!(!owner.start_with(
            expected.clone(),
            Kind::Change(Mode::Off),
            None,
            forbidden,
            no_wake
        ));
        assert_eq!(Preferences::capture(&config), expected);
    }
    release.send(())?;
    wake.recv_timeout(Duration::from_secs(5))?;
    let completed = owner.poll(&mut config, &mut running).unwrap();
    assert!(completed.changed && completed.restore.is_none());
    assert!(running && config.run_at_startup && !owner.busy());
    assert_eq!(config.ui_language, "ko");
    assert!(owner.poll(&mut config, &mut running).is_none());
    Ok(())
}

#[test]
fn reset_continuation_requires_success_and_the_exact_startup_snapshot() -> Result<()> {
    for success in [false, true] {
        for stale in [false, true] {
            let mut owner = Startup::disabled();
            let mut config = Config {
                run_as_admin_on_startup: true,
                start_in_tray: true,
                ..Config::default()
            };
            let mut running = true;
            let before = Preferences::capture(&config);
            let mut result = before.clone();
            result.set_mode(Mode::Off, Path::new("current.exe"));
            let selection = RestoreDefaultsSelection::default();
            let (woke, wake) = mpsc::sync_channel(1);
            assert!(owner.start_with(
                before.clone(),
                Kind::Restore(selection),
                None,
                move || {
                    if success {
                        Outcome {
                            running: false,
                            ..outcome(result)
                        }
                    } else {
                        reconcile::failed(result, true, Issue::UncertainState)
                    }
                },
                move || woke.send(()).unwrap()
            ));
            if stale {
                config.authorized_startup_path = "new selection".into();
            }
            wake.recv_timeout(Duration::from_secs(5))?;
            let completed = owner.poll(&mut config, &mut running).unwrap();
            assert_eq!(completed.restore, (success && !stale).then_some(selection));
            assert_eq!(completed.restore_failed, !success || stale);
            assert!(!completed.changed);
            assert_eq!(running, !success || stale);
            assert!(config.run_as_admin_on_startup && config.start_in_tray);
            if !success || stale {
                assert_eq!(owner.issue(), Some(Issue::UncertainState));
            }
            assert_eq!(
                config.authorized_startup_path,
                if stale {
                    "new selection"
                } else {
                    &before.authorized
                }
            );
        }
    }
    Ok(())
}

#[test]
fn panics_and_disconnected_workers_are_uncertain_and_never_commit_preferences() -> Result<()> {
    let mut owner = Startup::disabled();
    let mut config = Config::default();
    let mut running = true;
    let before = Preferences::capture(&config);
    let (woke, wake) = mpsc::sync_channel(1);
    assert!(owner.start_with(
        before.clone(),
        Kind::Change(Mode::Off),
        None,
        || panic!("worker fixture failure"),
        move || woke.send(()).unwrap()
    ));
    wake.recv_timeout(Duration::from_secs(5))?;
    owner.poll(&mut config, &mut running).unwrap();
    assert_eq!(owner.issue(), Some(Issue::UncertainState));
    assert_eq!(Preferences::capture(&config), before);
    assert!(running);
    let (sender, receiver) = mpsc::channel();
    drop(sender);
    owner.pending = Some(Pending {
        expected: before.clone(),
        kind: Kind::Change(Mode::Normal),
        receiver,
        permit: None,
    });
    owner.poll(&mut config, &mut running).unwrap();
    assert_eq!(owner.issue(), Some(Issue::UncertainState));
    assert_eq!(Preferences::capture(&config), before);
    assert!(running && !owner.busy());
    Ok(())
}

#[test]
fn dropping_the_owner_does_not_deliver_late_results_to_a_replacement() -> Result<()> {
    let mut owner = Startup::disabled();
    let before = preferences();
    let result = before.clone();
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let (woke, wake) = mpsc::sync_channel(1);
    assert!(owner.start_with(
        before,
        Kind::Change(Mode::Normal),
        None,
        move || {
            entered.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            outcome(result)
        },
        move || woke.send(()).unwrap()
    ));
    entry.recv_timeout(Duration::from_secs(5))?;
    drop(owner);
    let replacement = Startup::disabled();
    release.send(())?;
    wake.recv_timeout(Duration::from_secs(5))?;
    assert!(!replacement.busy() && replacement.issue().is_none());
    Ok(())
}

#[test]
fn native_lane_serializes_workers_and_contains_panics_without_poisoning() -> Result<()> {
    let lane = Arc::new(Mutex::new(()));
    let first_lane = lane.clone();
    let retired = Arc::new(AtomicBool::new(false));
    let first_retired = retired.clone();
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let first = std::thread::spawn(move || {
        serialized(&first_lane, preferences(), false, || {
            entered.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            first_retired.store(true, Ordering::SeqCst);
            panic!("native fixture failure")
        })
    });
    entry.recv_timeout(Duration::from_secs(5))?;
    assert!(lane.try_lock().is_err());
    let second_lane = lane.clone();
    let (attempted, attempt) = mpsc::sync_channel(1);
    let second = std::thread::spawn(move || {
        attempted.send(()).unwrap();
        serialized(&second_lane, preferences(), false, || {
            assert!(retired.load(Ordering::SeqCst));
            outcome(preferences())
        })
    });
    attempt.recv_timeout(Duration::from_secs(5))?;
    release.send(())?;
    assert_eq!(first.join().unwrap().issue, Some(Issue::UncertainState));
    assert!(second.join().unwrap().success);
    assert!(!lane.is_poisoned());
    Ok(())
}

#[test]
fn executable_discovery_failure_never_becomes_an_isolated_reset_bypass() {
    let isolated = Startup::disabled();
    assert!(isolated.system_disabled() && !isolated.available());
    let mut failed = Startup::with_path(Err(std::io::Error::other("executable fixture failure")));
    assert!(!failed.system_disabled() && !failed.available());
    assert_eq!(failed.issue(), Some(Issue::OperationFailed));
    assert!(!failed.request_restore(
        &Config::default(),
        false,
        RestoreDefaultsSelection::default(),
        &egui::Context::default()
    ));
    assert!(!failed.busy());
}

#[test]
fn unrepresentable_native_paths_do_not_replace_durable_authorization_with_lossy_text() {
    use std::os::windows::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
    ]));
    let mut preferences = preferences();
    preferences.authorized = "prior".into();
    preferences.set_mode(Mode::Normal, &path);
    assert!(preferences.normal && !preferences.elevated);
    assert_eq!(preferences.authorized, "prior");
}

#[test]
fn reset_rejects_inconsistent_success_before_destructive_continuation() -> Result<()> {
    for issue in [None, Some(Issue::UncertainState)] {
        let mut owner = Startup::disabled();
        let mut config = Config::default();
        let mut running = true;
        let before = Preferences::capture(&config);
        let result = Outcome {
            running: issue.is_none(),
            issue,
            ..outcome(before.clone())
        };
        let (woke, wake) = mpsc::sync_channel(1);
        assert!(owner.start_with(
            before.clone(),
            Kind::Restore(RestoreDefaultsSelection::default()),
            None,
            move || result,
            move || woke.send(()).unwrap()
        ));
        wake.recv_timeout(Duration::from_secs(5))?;
        let completed = owner.poll(&mut config, &mut running).unwrap();
        assert!(completed.restore.is_none() && completed.restore_failed && !completed.changed);
        assert_eq!(owner.issue(), Some(Issue::UncertainState));
        assert_eq!(Preferences::capture(&config), before);
        assert!(running);
    }
    Ok(())
}
