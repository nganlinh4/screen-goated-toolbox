use super::*;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};

fn candidate(version: &str) -> UpdateCandidate {
    UpdateCandidate {
        version: semver::Version::parse(version).unwrap(),
        body: "Notes".into(),
        asset_name: format!("ScreenGoatedToolbox_v{version}.exe"),
        download_url: format!(
            "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v{version}/ScreenGoatedToolbox_v{version}.exe"
        ),
        size_bytes: 1,
        sha256: "1".repeat(64),
    }
}

fn available_candidate() -> UpdateCandidate {
    let mut version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    version.major = version.major.checked_add(1).unwrap();
    version.minor = 0;
    version.patch = 0;
    candidate(&version.to_string())
}

fn staged() -> StagedUpdate {
    let checked = available_candidate();
    StagedUpdate {
        version: checked.version.to_string(),
        asset_name: checked.asset_name,
        size_bytes: checked.size_bytes,
        sha256: checked.sha256,
    }
}

fn latest(owner: &UpdateOwner, rx: &Receiver<UpdateEvent>) -> Option<UpdateStatus> {
    rx.try_iter().filter_map(|event| owner.accept(event)).last()
}

#[test]
fn queued_update_result_cannot_notify_after_a_newer_check_starts() {
    let (tx, rx) = channel();
    let owner = UpdateOwner::new(tx);
    let first = owner.begin_check(false).unwrap();
    owner.finish_check(&first, Ok(available_candidate()));
    owner.begin_check(false).unwrap();
    assert!(matches!(latest(&owner, &rx), Some(UpdateStatus::Checking)));
}

#[test]
fn selected_candidate_can_be_consumed_only_once() {
    let (tx, _) = channel();
    let owner = UpdateOwner::new(tx);
    let check = owner.begin_check(false).unwrap();
    owner.finish_check(&check, Ok(available_candidate()));
    assert!(owner.begin_download().is_some());
    assert!(owner.begin_download().is_none());
}

#[test]
fn stale_check_generation_cannot_publish_or_replace_selection() {
    let (tx, rx) = channel();
    let owner = UpdateOwner::new(tx);
    let first = owner.begin_check(false).unwrap();
    owner.finish_check(&first, Err(anyhow::anyhow!("unavailable")));
    owner.begin_check(false).unwrap();
    owner.finish_check(&first, Ok(available_candidate()));
    assert!(owner.state.lock().unwrap().candidate.is_none());
    assert!(matches!(latest(&owner, &rx), Some(UpdateStatus::Checking)));
}

#[test]
fn shared_update_lifecycle_timelines_match_windows_owner() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/app-update/lifecycle.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        if !case["platforms"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "windows")
        {
            continue;
        }
        let (tx, rx) = channel();
        let owner = UpdateOwner::new(tx);
        let mut operations = HashMap::new();
        for step in case["steps"].as_array().unwrap() {
            let id = step["id"].as_str().unwrap_or("");
            match step["action"].as_str().unwrap() {
                "check" | "auto" => {
                    let operation = owner.begin_check(step["action"] == "auto");
                    assert_eq!(
                        operation.is_some(),
                        step["accepted"].as_bool().unwrap(),
                        "{}",
                        case["name"]
                    );
                    if let Some(operation) = operation {
                        operations.insert(id, operation);
                    }
                }
                "available" => owner.finish_check(&operations[id], Ok(available_candidate())),
                "up_to_date" => {
                    owner.finish_check(&operations[id], Ok(candidate(env!("CARGO_PKG_VERSION"))))
                }
                "error" => owner.finish_check(&operations[id], Err(anyhow::anyhow!("unavailable"))),
                "start" => {
                    let download = owner.begin_download();
                    assert_eq!(
                        download.is_some(),
                        step["accepted"].as_bool().unwrap(),
                        "{}",
                        case["name"]
                    );
                    if let Some((operation, _)) = download {
                        operations.insert(id, operation);
                    }
                }
                "ready" => owner.finish_download(&operations[id], Ok(staged())),
                "notice" => assert_eq!(
                    matches!(
                        latest(&owner, &rx),
                        Some(UpdateStatus::UpdateAvailable { .. })
                    ),
                    step["accepted"].as_bool().unwrap(),
                    "{}",
                    case["name"]
                ),
                _ => panic!("unknown update lifecycle action"),
            }
        }
        let name = match owner.state.lock().unwrap().phase {
            Phase::Idle => "idle",
            Phase::Checking => "checking",
            Phase::Available => "available",
            Phase::UpToDate => "up_to_date",
            Phase::Downloading => "downloading",
            Phase::Ready => "ready",
            Phase::Error => "error",
            Phase::Closed => "closed",
        };
        assert_eq!(name, case["status"].as_str().unwrap(), "{}", case["name"]);
    }
}

#[test]
fn closed_owner_rejects_queued_and_late_results() {
    let (tx, rx) = channel();
    let owner = UpdateOwner::new(tx);
    let check = owner.begin_check(false).unwrap();
    owner.close();
    owner.finish_check(&check, Ok(available_candidate()));
    assert!(latest(&owner, &rx).is_none());
    assert!(owner.begin_check(false).is_none());
}

#[test]
fn events_are_current_owner_scoped_and_consumed_once() {
    let (tx, rx) = channel();
    let owner = UpdateOwner::new(tx);
    let (other_tx, _) = channel();
    let other = UpdateOwner::new(other_tx);
    owner.begin_check(false).unwrap();
    let event = rx.recv().unwrap();
    assert!(other.accept(event.clone()).is_none());
    assert!(owner.accept(event.clone()).is_some());
    assert!(owner.accept(event).is_none());
}

#[test]
fn concurrent_check_admission_has_one_owner() {
    let (tx, _) = channel();
    let owner = Arc::new(UpdateOwner::new(tx));
    let barrier = Arc::new(std::sync::Barrier::new(9));
    let threads = (0..8)
        .map(|_| {
            let owner = Arc::clone(&owner);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                owner.begin_check(false).is_some()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|admitted| *admitted)
            .count(),
        1
    );
}

#[test]
fn failed_download_requires_a_fresh_checked_action() {
    let (tx, _) = channel();
    let owner = UpdateOwner::new(tx);
    let check = owner.begin_check(false).unwrap();
    owner.finish_check(&check, Ok(available_candidate()));
    let (download, _) = owner.begin_download().unwrap();
    owner.finish_download(&download, Err(anyhow::anyhow!("unavailable")));
    assert!(owner.begin_download().is_none());
    assert!(owner.begin_check(false).is_some());
}
