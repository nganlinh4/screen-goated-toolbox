use super::*;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static THAWED: AtomicBool = AtomicBool::new(false);

fn record_thaw() {
    THAWED.store(true, Ordering::SeqCst);
}

#[test]
fn abandoned_preparation_releases_gate_and_thaws_draft() {
    THAWED.store(false, Ordering::SeqCst);
    let guard = ExitPreparation {
        _lock: TEST_LOCK.lock().unwrap(),
        _settings: None,
        _html_save: None,
        thaw: record_thaw,
        _drain: None,
    };
    assert!(TEST_LOCK.try_lock().is_err());
    drop(guard);
    assert!(THAWED.load(Ordering::SeqCst));
    assert!(TEST_LOCK.try_lock().is_ok());
}

#[test]
fn preparation_stops_on_failure_and_finishes_before_launch() {
    let events = RefCell::new(Vec::new());
    let failure = prepare_exit_steps(
        || {
            events.borrow_mut().push("recorder");
            Ok(())
        },
        || {
            events.borrow_mut().push("draft");
            Err::<(), _>(std::io::Error::other("unavailable"))
        },
    );
    if failure.is_ok() {
        events.borrow_mut().push("launch");
    }
    assert!(matches!(failure, Err(ExitPreparationError::Draft(_))));
    assert_eq!(*events.borrow(), ["recorder", "draft"]);

    events.borrow_mut().clear();
    let success = prepare_exit_steps(
        || {
            events.borrow_mut().push("recorder");
            Ok(())
        },
        || {
            events.borrow_mut().push("draft");
            Ok(())
        },
    );
    if success.is_ok() {
        events.borrow_mut().push("launch");
    }
    assert_eq!(*events.borrow(), ["recorder", "draft", "launch"]);
}

#[test]
fn activation_detected_after_preparation_releases_guard() {
    struct Guard<'a>(&'a AtomicBool);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let released = AtomicBool::new(false);
    let result = reject_startup_activation(Guard(&released), true);
    assert!(matches!(result, Err(RestartPrepareDenied::Blocked)));
    assert!(released.load(Ordering::SeqCst));
}

/// Quit, restart and update all leave through `exit_prepared`, so none skips
/// hiding the tray icon or retiring the compositors before `process::exit`.
#[test]
fn every_settings_quit_path_releases_owned_resources() {
    fn visit(dir: &std::path::Path, offenders: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, offenders);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && !path.ends_with("exit_preparation.rs")
                && !path.ends_with("exit_preparation_tests.rs")
                && std::fs::read_to_string(&path).is_ok_and(|source| {
                    source.contains("crash_report::exit(") || source.contains("process::exit(")
                })
            {
                offenders.push(path.display().to_string());
            }
        }
    }
    let mut offenders = Vec::new();
    visit(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gui"),
        &mut offenders,
    );
    assert!(
        offenders.is_empty(),
        "exits that skip release: {offenders:?}"
    );
    let source = include_str!("exit_preparation.rs");
    let exit = &source[source.find("pub(crate) fn exit_prepared").unwrap()..];
    let exit = &exit[..exit.find("crash_report::exit(").unwrap()];
    assert!(exit.contains("release_owned_resources();"));
}
