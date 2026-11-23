use super::*;
use std::cell::Cell;

#[test]
fn recorder_stop_failure_refreshes_status_and_reports_error() {
    let status = Mutex::new(InstallStatus::Installed);
    let logs = Mutex::new(Vec::new());
    let called = Cell::new(false);
    let result = run_tool_removal(
        &status,
        &logs,
        "tool",
        || Err::<(), _>(anyhow::anyhow!("stop failed")),
        || {
            called.set(true);
            Ok(())
        },
        || InstallStatus::Installed,
    );
    assert!(result.is_err());
    assert!(!called.get());
    assert_eq!(*status.lock().unwrap(), InstallStatus::Installed);
    assert!(logs.lock().unwrap()[0].contains("stop failed"));
}

#[test]
fn completed_and_failed_removals_leave_a_terminal_state() {
    let status = Mutex::new(InstallStatus::Installed);
    let logs = Mutex::new(Vec::new());
    assert!(
        run_tool_removal(
            &status,
            &logs,
            "tool",
            || Ok(()),
            || Ok(()),
            || unreachable!()
        )
        .is_ok()
    );
    assert_eq!(*status.lock().unwrap(), InstallStatus::Missing);
    assert!(
        run_tool_removal(
            &status,
            &logs,
            "tool",
            || Ok(()),
            || Err(anyhow::anyhow!("remove failed")),
            || InstallStatus::Error("verification failed".into())
        )
        .is_err()
    );
    assert_eq!(
        *status.lock().unwrap(),
        InstallStatus::Error("verification failed".into())
    );
}

#[test]
fn recorder_guard_outlives_the_tool_removal() {
    struct Guard<'a>(&'a Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let status = Mutex::new(InstallStatus::Installed);
    let logs = Mutex::new(Vec::new());
    let active = Cell::new(false);
    assert!(
        run_tool_removal(
            &status,
            &logs,
            "tool",
            || {
                active.set(true);
                Ok(Guard(&active))
            },
            || {
                assert!(active.get());
                Ok(())
            },
            || unreachable!(),
        )
        .is_ok()
    );
    assert!(!active.get());
}

#[test]
fn panicking_removal_and_refresh_leave_terminal_error() {
    let status = Mutex::new(InstallStatus::Installed);
    let logs = Mutex::new(Vec::new());
    let result = run_tool_removal(
        &status,
        &logs,
        "tool",
        || Ok(()),
        || panic!("injected removal panic"),
        || panic!("injected refresh panic"),
    );
    assert!(result.is_err());
    assert!(matches!(*status.lock().unwrap(), InstallStatus::Error(_)));
    assert!(!logs.lock().unwrap().is_empty());
}

#[test]
fn pending_removal_is_reported_as_attention() {
    let status = Mutex::new(InstallStatus::Installed);
    let logs = Mutex::new(Vec::new());
    let result = run_tool_removal(
        &status,
        &logs,
        "tool",
        || Ok(()),
        || external_tool_removal_outcome("tool", RemovalOutcome::Pending),
        || InstallStatus::Installed,
    );
    assert!(result.is_err());
    assert_eq!(*status.lock().unwrap(), InstallStatus::Installed);
    assert!(logs.lock().unwrap()[0].contains("pending"));
}

#[test]
fn a_failed_tool_offers_retry_and_removal() {
    let text = LocaleText::get("en");
    let error = InstallStatus::Error("bad archive".into());
    assert_eq!(
        tool_state(&error, &text, ExternalTool::YtDlp).offered_actions(),
        [RowAction::Install, RowAction::Remove]
    );
    let installed = InstallStatus::Installed;
    assert_eq!(
        tool_state(&installed, &text, ExternalTool::Ffmpeg).offered_actions(),
        [RowAction::Remove]
    );
    let missing = InstallStatus::Missing;
    assert_eq!(
        tool_state(&missing, &text, ExternalTool::Deno).offered_actions(),
        [RowAction::Install]
    );
}
