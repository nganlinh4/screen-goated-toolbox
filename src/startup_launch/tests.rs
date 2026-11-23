use super::*;
use std::ffi::OsString;

fn args(arguments: &[&str]) -> Vec<OsString> {
    std::iter::once("application.exe")
        .chain(arguments.iter().copied())
        .map(OsString::from)
        .collect()
}

#[test]
fn late_and_unknown_uptime_never_query_startup_registration() {
    for uptime in [None, Some(180), Some(181), Some(u64::MAX)] {
        assert_eq!(
            autostart_reason(&args(&[]), uptime, || panic!(
                "ineligible inference queried registration"
            )),
            None
        );
    }
}

#[test]
fn eligible_legacy_inference_observes_registration_once_and_preserves_its_result() {
    for uptime in [0, 1, 179] {
        for registered in [false, true] {
            let mut calls = 0;
            let result = autostart_reason(&args(&[]), Some(uptime), || {
                calls += 1;
                registered
            });
            assert_eq!(calls, 1);
            assert_eq!(result, registered.then_some(Autostart::Legacy));
        }
    }
}

#[test]
fn explicit_autostart_does_not_depend_on_uptime_or_registration() {
    for uptime in [None, Some(0), Some(180), Some(u64::MAX)] {
        for arguments in [vec![AUTOSTART_ARG], vec![AUTOSTART_ARG, AUTOSTART_ARG]] {
            assert_eq!(
                autostart_reason(&args(&arguments), uptime, || panic!(
                    "explicit launch queried registration"
                )),
                Some(Autostart::Explicit)
            );
        }
    }
}

#[test]
fn other_arguments_and_missing_argv_do_not_infer_or_replay_autostart() {
    for arguments in [
        vec!["document.txt"],
        vec!["--restarted"],
        vec![AUTOSTART_ARG, "document.txt"],
        vec!["--other-option", AUTOSTART_ARG],
    ] {
        assert_eq!(
            autostart_reason(&args(&arguments), Some(0), || panic!(
                "unrelated launch queried registration"
            )),
            None
        );
    }
    assert_eq!(
        autostart_reason(&[], Some(0), || panic!("missing argv queried registration")),
        None
    );
}
