use super::super::RuntimeState;
use super::*;
use crate::overlay::creation_submission_test::queued::*;

#[test]
fn queued_recovery_retains_unavailable_vector_requests_and_recovers_once() {
    let name = format!(
        "{}::queued_recovery_retains_unavailable_vector_requests_and_recovers_once",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated_without_fault(&name, |root| {
        for case in fixture()["availabilityCases"].as_array().unwrap() {
            let saved = (0..2)
                .map(|_| {
                    seed(
                        root,
                        "svg",
                        "svg",
                        true,
                        serde_json::json!({"model":"simple","backgroundMode":"opaque"}),
                        |args| {
                            serde_json::to_value(
                                serde_json::from_value::<StartJobRequest>(args).unwrap(),
                            )
                            .unwrap()
                        },
                    )
                })
                .collect::<Vec<_>>();
            let offline = case["available"]
                .as_array()
                .unwrap()
                .iter()
                .position(|value| value == false)
                .unwrap();
            let moved = move_destination(&saved[offline]);
            let before = journal_bytes();
            recover_pending(|| {});
            assert!(
                journal_bytes() == before,
                "Unavailable storage retired a frozen accepted request"
            );
            let state = STATE.lock().unwrap();
            let queued = saved
                .iter()
                .enumerate()
                .filter_map(|(index, record)| {
                    state.jobs.contains_key(&record.job_id).then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(serde_json::json!(queued), case["expectedQueued"]);
            drop(state);
            std::fs::rename(moved, &saved[offline].destination).unwrap();
            recover_pending(|| {});
            let state = STATE.lock().unwrap();
            assert_eq!(state.jobs.len(), 2);
            assert_eq!(
                state.order.len(),
                2,
                "Recovery registered the same dispatch twice"
            );
            assert!(journal_bytes() == before);
            drop(state);
            for record in &saved {
                crate::overlay::creation_intent_journal::clear_required("svg", &record.job_id)
                    .unwrap();
            }
            *STATE.lock().unwrap() = RuntimeState::default();
        }
    });
}

#[test]
fn queued_recovery_expires_svg_requests_without_the_destination() {
    let name = format!(
        "{}::queued_recovery_expires_svg_requests_without_the_destination",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated_without_fault(&name, |root| {
        let saved = seed(
            root,
            "svg",
            "svg",
            true,
            serde_json::json!({"model":"simple","backgroundMode":"opaque"}),
            |args| {
                serde_json::to_value(serde_json::from_value::<StartJobRequest>(args).unwrap())
                    .unwrap()
            },
        );
        move_destination(&saved);
        expire_requests();
        let intent = crate::overlay::creation_intent_journal::load("svg")
            .unwrap()
            .remove(0);
        assert!(
            crate::overlay::creation_process_supervisor::deadline_expired(intent.deadline_at_ms)
        );
        assert!(recover_pending(|| {}));
        assert!(
            crate::overlay::creation_intent_journal::load("svg")
                .unwrap()
                .is_empty()
        );
        let state = STATE.lock().unwrap();
        let stage = &state.jobs.get(&saved.job_id).unwrap().stage;
        let contract = fixture();
        let case = contract["transitions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["deadlineExpired"] == true)
            .unwrap();
        assert_eq!(stage, case["expected"].as_str().unwrap());
    });
}

#[test]
fn queued_recovery_shutdown_waits_for_svg_admission_ownership() {
    let name = format!(
        "{}::queued_recovery_shutdown_waits_for_svg_admission_ownership",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated_without_fault(&name, |root| {
        let _saved = seed(
            root,
            "svg",
            "svg",
            true,
            serde_json::json!({"model":"simple","backgroundMode":"opaque"}),
            |args| {
                serde_json::to_value(serde_json::from_value::<StartJobRequest>(args).unwrap())
                    .unwrap()
            },
        );
        let before = journal_bytes();
        let admission = ADMISSION.acquire();
        assert!(!super::super::super::shutdown());
        assert!(journal_bytes() == before);
        drop(admission);
        assert!(super::super::super::shutdown());
        assert!(
            crate::overlay::creation_intent_journal::load("svg")
                .unwrap()
                .is_empty()
        );
    });
}
