use super::*;
use std::time::Duration;

fn picker() -> FolderPicker {
    FolderPicker::with_gate(Arc::new(AtomicBool::new(false)))
}

fn start_and_wait(picker: &mut FolderPicker, result: PickResult) -> PickResult {
    let (wake_tx, wake_rx) = mpsc::channel();
    picker
        .start_with(
            move || result,
            move || {
                let _ = wake_tx.send(());
            },
        )
        .unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    picker.poll().unwrap()
}

#[test]
fn selection_cancellation_failure_and_unicode_follow_shared_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/video-downloader/folder-picker.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let case = |name: &str| cases.iter().find(|item| item["event"] == name).unwrap();
    let mut picker = picker();
    let path = PathBuf::from("C:\\한글\\café");
    let selected = start_and_wait(&mut picker, Ok(Some(path.clone()))).unwrap();
    assert_eq!(selected.as_ref(), Some(&path));
    assert_eq!(case("selected_current")["apply"], true);
    assert_eq!(case("unicode_path")["preserve_path"], true);
    assert_eq!(start_and_wait(&mut picker, Ok(None)).unwrap(), None);
    assert_eq!(case("cancelled")["apply"], false);
    assert!(start_and_wait(&mut picker, Err("injected failure".into())).is_err());
    assert_eq!(case("failed")["apply"], false);
}

#[test]
fn retired_and_replaced_owners_cannot_apply_late_results() {
    let gate = Arc::new(AtomicBool::new(false));
    let mut old = FolderPicker::with_gate(gate.clone());
    let mut replacement = FolderPicker::with_gate(gate);
    let (release_tx, release_rx) = mpsc::channel();
    let (wake_tx, wake_rx) = mpsc::channel();
    old.start_with(
        move || {
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok(Some(PathBuf::from("old")))
        },
        move || {
            let _ = wake_tx.send(());
        },
    )
    .unwrap();
    assert!(!replacement.available());
    assert!(replacement.start_with(|| Ok(None), || {}).is_err());
    old.retire();
    assert!(!replacement.available());
    release_tx.send(()).unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(old.poll().is_none());
    assert!(replacement.available());
    assert_eq!(
        start_and_wait(&mut replacement, Ok(Some(PathBuf::from("new")))).unwrap(),
        Some(PathBuf::from("new"))
    );
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/video-downloader/folder-picker.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for name in ["selected_after_close", "selected_after_replacement"] {
        assert_eq!(
            cases.iter().find(|item| item["event"] == name).unwrap()["apply"],
            false
        );
    }
    assert_eq!(
        cases
            .iter()
            .find(|item| item["event"] == "repeated_open")
            .unwrap()["overlap_allowed"],
        false
    );
}

#[test]
fn panicking_picker_releases_its_slot_and_reports_failure() {
    let mut picker = picker();
    let (wake_tx, wake_rx) = mpsc::channel();
    picker
        .start_with(
            || panic!("injected picker failure"),
            move || {
                let _ = wake_tx.send(());
            },
        )
        .unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(picker.poll().unwrap().is_err());
    assert!(picker.available());
}

#[test]
fn failed_persistence_keeps_the_prior_destination() {
    let previous = PathBuf::from("C:\\previous");
    let selected = PathBuf::from("C:\\한글");
    let mut current = Some(previous.clone());
    assert!(
        commit_choice(&mut current, selected.clone(), |_| Err(
            std::io::Error::other("injected")
        ))
        .is_err()
    );
    assert_eq!(current, Some(previous));
    commit_choice(&mut current, selected.clone(), |_| Ok(())).unwrap();
    assert_eq!(current, Some(selected));
}
