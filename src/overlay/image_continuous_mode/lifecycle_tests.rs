use super::*;

fn request(id: usize) -> Request {
    Request {
        target: ImageCaptureTarget::Preset(id),
        hotkey_name: "test".into(),
        hotkey_id: 1,
        trigger_vk: 1,
    }
}

fn start(admission: Admission) -> u64 {
    match admission {
        Admission::Start(id, _) => id,
        _ => panic!("expected start"),
    }
}

#[test]
fn stop_before_queue_cannot_publish_a_worker() {
    let mut state = Lifecycle::default();
    let id = start(state.enter(request(1)));
    assert_eq!(state.stop(), Some(0));
    assert!(!state.register_thread(id, 42));
    assert!(!state.ready(id));
    assert!(matches!(state.finish(id), Admission::Ignored));
    assert!(!state.live(id));
}

#[test]
fn replacement_waits_until_owner_finishes_and_stale_retirement_is_ignored() {
    let mut state = Lifecycle::default();
    let first = start(state.enter(request(1)));
    assert_eq!(
        state.desired_binding().unwrap().0,
        ImageCaptureTarget::Preset(1)
    );
    assert!(state.register_thread(first, 42));
    assert!(state.ready(first));
    assert_eq!(state.stop(), Some(42));
    assert!(matches!(state.enter(request(2)), Admission::Queued));
    assert_eq!(
        state.desired_binding().unwrap().0,
        ImageCaptureTarget::Preset(2)
    );
    assert!(!state.desired_binding().unwrap().2);
    assert!(!state.live(first));
    let second = start(state.finish(first));
    assert_ne!(first, second);
    assert!(matches!(state.finish(first), Admission::Ignored));
    assert!(state.live(second));
    assert!(state.ready(second));
}

#[test]
fn duplicate_entry_and_cancelled_pending_do_not_start_more_workers() {
    let mut state = Lifecycle::default();
    let first = start(state.enter(request(1)));
    assert!(matches!(state.enter(request(2)), Admission::Ignored));
    assert_eq!(state.stop(), Some(0));
    assert!(matches!(state.enter(request(3)), Admission::Queued));
    assert_eq!(state.stop(), None);
    assert!(matches!(state.finish(first), Admission::Ignored));
}

#[test]
fn startup_failure_retires_owner_without_ready_publication() {
    let mut state = Lifecycle::default();
    let first = start(state.enter(request(1)));
    assert!(state.register_thread(first, 11));
    assert!(matches!(state.finish(first), Admission::Ignored));
    assert!(!state.running());
    assert!(!state.live(first));
    assert!(matches!(state.enter(request(2)), Admission::Start(..)));
}

#[test]
fn stop_and_down_admission_preserve_the_button_pair() {
    let mut state = Lifecycle::default();
    let first = start(state.enter(request(1)));
    assert!(state.ready(first));
    let gesture = super::super::gesture::Gesture::new();
    assert_eq!(state.begin_gesture(&gesture), Some(true));
    assert_eq!(gesture.cancel(), super::super::gesture::Cancel::Cancelled);
    assert!(state.running());
    assert_eq!(gesture.release(), super::super::gesture::Release::Cancelled);
    assert_eq!(state.stop(), Some(0));
    assert_eq!(state.begin_gesture(&gesture), None);
}

#[test]
fn shared_fixture_states_the_owner_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/preset-system/continuous-image-lifecycle.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let find = |name: &str| cases.iter().find(|case| case["event"] == name).unwrap();
    assert_eq!(find("stop_during_startup")["old_input_allowed"], false);
    assert_eq!(find("startup_failure")["active_after_retirement"], false);
    assert_eq!(
        find("replacement_during_retirement")["overlap_allowed"],
        false
    );
    assert_eq!(find("stale_retirement")["replacement_remains_active"], true);
    assert_eq!(find("stop_active_drag")["first_stop"], "cancel_drag");
}
