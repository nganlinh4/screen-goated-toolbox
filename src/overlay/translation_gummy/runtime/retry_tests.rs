use super::*;

#[test]
fn replaced_or_stopped_owner_cannot_publish() {
    let old = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let new = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let current = SessionControl {
        stop: new.clone(),
        playback_ids: Arc::new(Mutex::new(Vec::new())),
    };
    assert!(!accepts_owner(Some(&current), &old));
    assert!(accepts_owner(Some(&current), &new));
    new.store(true, Ordering::SeqCst);
    assert!(!accepts_owner(Some(&current), &new));
    assert!(!accepts_owner(None, &old));
}

#[test]
fn failure_classification_matches_shared_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/translation-gummy/reconnect-contract.json"
    ))
    .unwrap();
    for case in fixture["failureCases"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        let failure: anyhow::Error = match kind {
            "close" => SessionFailure::Close(case["code"].as_u64().unwrap() as u16).into(),
            "server" => SessionFailure::Server {
                retryable: case["retryable"].as_bool().unwrap(),
            }
            .into(),
            "transport" => continue,
            _ => panic!("unexpected fixture kind"),
        };
        assert_eq!(
            retryable(&failure),
            case["action"] == "reconnect",
            "{}",
            case["name"]
        );
    }
}

#[test]
fn retries_are_bounded_and_setup_does_not_reset_them() {
    let mut lifecycle = new_lifecycle();
    lifecycle.reduce(0, LiveLifecycleEvent::Start);
    let mut now = 0;
    for attempt in 0..7 {
        let generation = lifecycle.state().generation;
        lifecycle.reduce(now, LiveLifecycleEvent::SocketOpened { generation });
        lifecycle.reduce(
            now,
            LiveLifecycleEvent::Frame(LiveLifecycleFrame {
                generation,
                setup_complete: true,
                ..Default::default()
            }),
        );
        let effects = lifecycle.reduce(
            now,
            LiveLifecycleEvent::TransportFailure {
                generation,
                retryable: true,
            },
        );
        let delay = effects.iter().find_map(|effect| {
            if let LiveLifecycleEffect::ScheduleReconnect { delay_ms, .. } = effect {
                Some(*delay_ms)
            } else {
                None
            }
        });
        if attempt == 6 {
            assert!(delay.is_none());
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, LiveLifecycleEffect::ReportFailure { .. }))
            );
        } else {
            now += delay.unwrap();
            lifecycle.reduce(now, LiveLifecycleEvent::Start);
        }
    }
}

#[test]
fn meaningful_content_resets_retry_budget() {
    let mut lifecycle = new_lifecycle();
    lifecycle.reduce(0, LiveLifecycleEvent::Start);
    let generation = lifecycle.state().generation;
    let effects = lifecycle.reduce(
        0,
        LiveLifecycleEvent::TransportFailure {
            generation,
            retryable: true,
        },
    );
    let delay = effects
        .iter()
        .find_map(|effect| {
            if let LiveLifecycleEffect::ScheduleReconnect { delay_ms, .. } = effect {
                Some(*delay_ms)
            } else {
                None
            }
        })
        .unwrap();
    lifecycle.reduce(delay, LiveLifecycleEvent::Start);
    let generation = lifecycle.state().generation;
    lifecycle.reduce(delay, LiveLifecycleEvent::SocketOpened { generation });
    lifecycle.reduce(
        delay,
        LiveLifecycleEvent::Frame(LiveLifecycleFrame {
            generation,
            setup_complete: true,
            ..Default::default()
        }),
    );
    assert_eq!(lifecycle.state().reconnect_attempt, 1);
    lifecycle.reduce(
        delay,
        LiveLifecycleEvent::Frame(LiveLifecycleFrame {
            generation,
            content_count: 1,
            ..Default::default()
        }),
    );
    assert_eq!(lifecycle.state().reconnect_attempt, 0);
}
