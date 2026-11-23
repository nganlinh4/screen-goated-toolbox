use super::*;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

mod unc_paths;
mod uncertain;

const SID: &str = "S-1-5-21-1-2-3-1001";
const CURRENT: &str = r"C:\Protected\app.exe";
const PRIOR: &str = r"C:\Portable\previous.exe";

fn snapshot(path: &str) -> TaskSnapshot {
    TaskSnapshot {
        target: PathBuf::from(path),
        args: crate::startup_launch::AUTOSTART_ARG.to_string(),
        working_directory: PathBuf::new(),
        principal_sid: SID.to_string(),
        highest: true,
        interactive: true,
        one_exec: true,
        one_logon: true,
        source: None,
        enabled: true,
        security_descriptor: format!("O:BAG:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;{SID})"),
    }
}

#[derive(Default)]
struct Mock {
    task: Option<TaskSnapshot>,
    reads: VecDeque<Option<TaskSnapshot>>,
    calls: Vec<&'static str>,
    fail_query: bool,
    query_count: usize,
    fail_query_at: Vec<usize>,
    fail_register: bool,
    fail_disable: bool,
    fail_delete: bool,
    bad_descriptor: bool,
    commit_then_fail: bool,
    sticky_register_failure: bool,
    preflight_error: Option<Issue>,
    guarded: Option<Arc<AtomicBool>>,
}

impl Scheduler for Mock {
    fn query(&mut self) -> Result<Option<TaskSnapshot>> {
        if let Some(guard) = &self.guarded {
            assert!(guard.load(Ordering::Acquire));
        }
        self.calls.push("query");
        self.query_count += 1;
        ensure!(!self.fail_query, "Task service unavailable");
        ensure!(
            !self.fail_query_at.contains(&self.query_count),
            "Task readback unavailable"
        );
        Ok(self.reads.pop_front().unwrap_or_else(|| self.task.clone()))
    }
    fn register(&mut self, target: &Path, user_sid: &str, update: bool) -> Result<()> {
        if let Some(guard) = &self.guarded {
            assert!(guard.load(Ordering::Acquire));
        }
        self.calls.push(if update { "update" } else { "create" });
        if let Some(issue) = self.preflight_error {
            return Err(anyhow::Error::from(issue).context(NotSubmitted));
        }
        if self.fail_register {
            let error = anyhow::anyhow!("Registration rejected");
            return Err(if self.sticky_register_failure {
                provider::uncertain(error)
            } else {
                error
            });
        }
        assert_eq!(self.task.is_some(), update);
        let mut task = snapshot(CURRENT);
        task.target = target.to_path_buf();
        task.working_directory = target.parent().unwrap().to_path_buf();
        task.principal_sid = user_sid.to_string();
        task.source = Some(SOURCE.to_string());
        if self.bad_descriptor {
            task.security_descriptor = format!("O:BAG:SYD:P(A;;FA;;;{SID})");
        }
        self.task = Some(task);
        if self.commit_then_fail {
            let error = anyhow::anyhow!("Task committed before RPC failed");
            return Err(if self.sticky_register_failure {
                provider::uncertain(error)
            } else {
                error
            });
        }
        Ok(())
    }
    fn disable(&mut self) -> Result<()> {
        self.calls.push("disable");
        ensure!(!self.fail_disable, "Disable rejected");
        self.task.as_mut().unwrap().enabled = false;
        Ok(())
    }
    fn delete(&mut self) -> Result<()> {
        self.calls.push("delete");
        ensure!(!self.fail_delete, "Delete rejected");
        self.task = None;
        Ok(())
    }
}

struct Guard {
    path: PathBuf,
    alive: Arc<AtomicBool>,
}
impl Protected for Guard {
    fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}
fn protected(path: &Path) -> Result<Guard> {
    Ok(Guard {
        path: path.to_path_buf(),
        alive: Arc::new(AtomicBool::new(true)),
    })
}
fn unsafe_target(_: &Path) -> Result<Guard> {
    anyhow::bail!("Untrusted owner or mutation access")
}
fn rejected_issue(error: anyhow::Error, expected: Issue) {
    assert_eq!(error.downcast_ref::<Issue>(), Some(&expected));
}

#[test]
fn unsafe_target_is_rejected_before_any_scheduler_call_or_existing_task_change() {
    let existing = snapshot(PRIOR);
    let mut backend = Mock {
        task: Some(existing.clone()),
        ..Mock::default()
    };
    rejected_issue(
        enable_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(PRIOR),
            SID,
            unsafe_target,
        )
        .unwrap_err(),
        Issue::ProtectedLocationRequired,
    );
    assert!(backend.calls.is_empty());
    assert_eq!(backend.task, Some(existing));
}

#[test]
fn protected_target_guard_survives_create_and_observed_registration() {
    let alive = Arc::new(AtomicBool::new(true));
    let mut backend = Mock {
        guarded: Some(alive.clone()),
        ..Mock::default()
    };
    enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(""),
        SID,
        |path| {
            Ok(Guard {
                path: path.to_path_buf(),
                alive: alive.clone(),
            })
        },
    )
    .unwrap();
    assert_eq!(backend.calls, ["query", "query", "create", "query"]);
    assert!(!alive.load(Ordering::Acquire));
    assert_eq!(backend.task.unwrap().source.as_deref(), Some(SOURCE));
}

#[test]
fn registration_failure_keeps_the_previous_task_and_releases_target_guards() {
    let existing = snapshot(PRIOR);
    let alive = Arc::new(AtomicBool::new(true));
    let mut backend = Mock {
        task: Some(existing.clone()),
        fail_register: true,
        guarded: Some(alive.clone()),
        ..Mock::default()
    };
    assert!(
        enable_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(PRIOR),
            SID,
            |path| Ok(Guard {
                path: path.to_path_buf(),
                alive: alive.clone()
            })
        )
        .is_err()
    );
    assert_eq!(backend.task, Some(existing));
    assert_eq!(backend.calls, ["query", "query", "update", "query"]);
    assert!(!alive.load(Ordering::Acquire));
}

#[test]
fn task_service_failure_is_not_treated_as_a_missing_registration() {
    let mut backend = Mock {
        fail_query: true,
        ..Mock::default()
    };
    assert!(
        enable_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            protected
        )
        .is_err()
    );
    assert_eq!(backend.calls, ["query"]);
}

#[test]
fn foreign_or_ambiguous_task_shapes_are_never_overwritten_or_retired() {
    let original = snapshot(CURRENT);
    let mut cases = Vec::new();
    for field in 0..10 {
        let mut task = original.clone();
        match field {
            0 => task.principal_sid = "S-1-5-21-1-2-3-1002".to_string(),
            1 => task.args.push_str(" --other-action"),
            2 => task.one_exec = false,
            3 => task.one_logon = false,
            4 => task.highest = false,
            5 => task.interactive = false,
            6 => task.source = Some("Different registration source".to_string()),
            7 => task.working_directory = PathBuf::from(r"C:\Other"),
            8 => task.target = PathBuf::from(r"C:\Other\app.exe"),
            _ => task.target = PathBuf::from(r"\\server\share\app.exe"),
        }
        cases.push(task);
    }
    for task in cases {
        let mut backend = Mock {
            task: Some(task.clone()),
            ..Mock::default()
        };
        rejected_issue(
            enable_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(""),
                SID,
                protected,
            )
            .unwrap_err(),
            Issue::TaskConflict,
        );
        rejected_issue(
            reconcile_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(""),
                SID,
                unsafe_target,
            )
            .unwrap_err(),
            Issue::TaskConflict,
        );
        rejected_issue(
            delete_using(&mut backend, Path::new(CURRENT), Path::new(""), SID).unwrap_err(),
            Issue::TaskConflict,
        );
        assert_eq!(backend.task, Some(task));
        assert!(backend.calls.iter().all(|call| *call == "query"));
    }
}

#[test]
fn changed_task_between_inspection_and_mutation_is_preserved() {
    for action in 0..3 {
        let initial = snapshot(CURRENT);
        let changed = snapshot(r"C:\Other\replacement.exe");
        let mut backend = Mock {
            task: Some(changed.clone()),
            reads: VecDeque::from([Some(initial)]),
            ..Mock::default()
        };
        let result = match action {
            0 => enable_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(""),
                SID,
                protected,
            ),
            1 => delete_using(&mut backend, Path::new(CURRENT), Path::new(""), SID),
            _ => reconcile_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(""),
                SID,
                unsafe_target,
            )
            .map(|_| ()),
        };
        rejected_issue(result.unwrap_err(), Issue::TaskConflict);
        assert_eq!(backend.task, Some(changed));
        assert_eq!(backend.calls, ["query", "query"]);
    }
}

#[test]
fn unsafe_legacy_prior_target_is_disabled_without_deleting_its_definition() {
    let original = snapshot(PRIOR);
    let mut backend = Mock {
        task: Some(original.clone()),
        ..Mock::default()
    };
    assert_eq!(
        reconcile_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(PRIOR),
            SID,
            unsafe_target
        )
        .unwrap(),
        Reconciled::Retired
    );
    let mut expected = original;
    expected.enabled = false;
    assert_eq!(backend.task, Some(expected));
    assert_eq!(backend.calls, ["query", "query", "disable", "query"]);
}

#[test]
fn stored_legacy_path_never_authorizes_executing_that_path() {
    let mut backend = Mock {
        task: Some(snapshot(PRIOR)),
        ..Mock::default()
    };
    enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(PRIOR),
        SID,
        protected,
    )
    .unwrap();
    assert_eq!(backend.task.unwrap().target, Path::new(CURRENT));
}

#[test]
fn failed_retirement_is_reported_and_keeps_the_original_definition() {
    let original = snapshot(PRIOR);
    let mut backend = Mock {
        task: Some(original.clone()),
        fail_disable: true,
        ..Mock::default()
    };
    assert!(
        reconcile_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(PRIOR),
            SID,
            unsafe_target
        )
        .is_err()
    );
    assert_eq!(backend.task, Some(original));
    assert!(!backend.calls.contains(&"delete"));
}

#[test]
fn disabled_and_protected_tasks_do_not_need_retirement() {
    let mut task = snapshot(CURRENT);
    task.enabled = false;
    let mut backend = Mock {
        task: Some(task),
        ..Mock::default()
    };
    assert_eq!(
        reconcile_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            |_| -> Result<Guard> { panic!("disabled task must stay untouched") }
        )
        .unwrap(),
        Reconciled::Disabled
    );
    backend.task.as_mut().unwrap().enabled = true;
    assert_eq!(
        reconcile_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            protected
        )
        .unwrap(),
        Reconciled::Enabled
    );
    assert_eq!(backend.calls, ["query", "query"]);
}

#[test]
fn explicit_off_deletes_only_an_exact_owned_task_and_verifies_absence() {
    let mut backend = Mock {
        task: Some(snapshot(PRIOR)),
        ..Mock::default()
    };
    delete_using(&mut backend, Path::new(CURRENT), Path::new(PRIOR), SID).unwrap();
    assert_eq!(backend.calls, ["query", "query", "delete", "query"]);
    assert!(backend.task.is_none());
    backend.task = Some(snapshot(CURRENT));
    backend.fail_delete = true;
    assert!(delete_using(&mut backend, Path::new(CURRENT), Path::new(""), SID).is_err());
    assert!(backend.task.is_some());
}

#[test]
fn an_unprotected_registration_readback_is_disabled_and_never_accepted() {
    let mut backend = Mock {
        bad_descriptor: true,
        ..Mock::default()
    };
    rejected_issue(
        enable_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            protected,
        )
        .unwrap_err(),
        Issue::OperationFailed,
    );
    assert!(!backend.task.as_ref().unwrap().enabled);
    assert_eq!(
        backend.calls,
        [
            "query", "query", "create", "query", "query", "disable", "query"
        ]
    );
}

#[test]
fn unprotected_legacy_task_permissions_require_retirement_even_for_protected_files() {
    let mut task = snapshot(CURRENT);
    task.security_descriptor = format!("O:{SID}D:P(A;;FA;;;{SID})");
    let mut backend = Mock {
        task: Some(task),
        ..Mock::default()
    };
    assert_eq!(
        reconcile_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(""),
            SID,
            protected
        )
        .unwrap(),
        Reconciled::Retired
    );
    assert!(!backend.task.unwrap().enabled);
}

#[test]
fn existing_protected_registration_is_verified_without_mutation() {
    let mut task = snapshot(CURRENT);
    task.source = Some(SOURCE.to_string());
    let mut backend = Mock {
        task: Some(task),
        fail_register: true,
        ..Mock::default()
    };
    enable_using(
        &mut backend,
        Path::new(CURRENT),
        Path::new(""),
        SID,
        protected,
    )
    .unwrap();
    assert_eq!(backend.calls, ["query", "query"]);
}

#[test]
fn a_disabled_owned_task_already_satisfies_off_without_delete_permission() {
    let mut task = snapshot(CURRENT);
    task.enabled = false;
    task.source = Some(SOURCE.to_string());
    let mut backend = Mock {
        task: Some(task.clone()),
        fail_delete: true,
        ..Mock::default()
    };
    delete_using(&mut backend, Path::new(CURRENT), Path::new(""), SID).unwrap();
    assert_eq!(backend.calls, ["query"]);
    assert_eq!(backend.task, Some(task));
}

#[test]
fn disabled_legacy_definitions_are_removed_before_prior_path_authority_can_be_cleared() {
    let mut task = snapshot(PRIOR);
    task.enabled = false;
    let mut backend = Mock {
        task: Some(task),
        ..Mock::default()
    };
    delete_using(&mut backend, Path::new(CURRENT), Path::new(PRIOR), SID).unwrap();
    assert_eq!(backend.calls, ["query", "query", "delete", "query"]);
    assert!(backend.task.is_none());
}
