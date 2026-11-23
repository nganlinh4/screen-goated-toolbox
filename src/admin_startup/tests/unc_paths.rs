use super::*;

const REMOTE: &str = r"\\example.invalid\share\app.exe";

#[test]
fn bound_legacy_and_marked_unc_tasks_are_retired_without_probing_the_remote_target() {
    for source in [None, Some(SOURCE.to_string())] {
        let mut task = snapshot(REMOTE);
        task.source = source;
        let mut backend = Mock {
            task: Some(task),
            ..Mock::default()
        };
        assert_eq!(
            reconcile_using(
                &mut backend,
                Path::new(CURRENT),
                Path::new(REMOTE),
                SID,
                |_| -> Result<Guard> {
                    panic!("remote target must not reach filesystem protection")
                }
            )
            .unwrap(),
            Reconciled::Retired
        );
        assert!(!backend.task.unwrap().enabled);
    }
}

#[test]
fn unbound_unc_legacy_tasks_are_preserved() {
    let task = snapshot(REMOTE);
    let mut backend = Mock {
        task: Some(task.clone()),
        ..Mock::default()
    };
    rejected_issue(
        reconcile_using(
            &mut backend,
            Path::new(CURRENT),
            Path::new(PRIOR),
            SID,
            |_| -> Result<Guard> { panic!("unbound task must not reach protection") },
        )
        .unwrap_err(),
        Issue::TaskConflict,
    );
    assert_eq!(backend.task, Some(task));
    assert_eq!(backend.calls, ["query"]);
}

#[test]
fn current_unc_executables_cannot_be_registered_for_elevated_startup() {
    let mut backend = Mock::default();
    rejected_issue(
        enable_using(
            &mut backend,
            Path::new(REMOTE),
            Path::new(REMOTE),
            SID,
            |_| -> Result<Guard> { panic!("network path must be rejected before protection") },
        )
        .unwrap_err(),
        Issue::ProtectedLocationRequired,
    );
    assert!(backend.calls.is_empty());
}
