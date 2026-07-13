use super::*;

#[test]
fn selected_policy_keeps_content_before_generation_boundary() {
    let mut lifecycle =
        LiveSessionLifecycle::new(continuous_policy(), LiveBackoffPolicy::default());
    lifecycle.reduce(0, LiveLifecycleEvent::Start);
    lifecycle.reduce(0, LiveLifecycleEvent::SocketOpened { generation: 1 });
    lifecycle.reduce(
        0,
        LiveLifecycleEvent::Frame(LiveLifecycleFrame {
            generation: 1,
            setup_complete: true,
            ..LiveLifecycleFrame::default()
        }),
    );

    let effects = lifecycle.reduce(
        1,
        LiveLifecycleEvent::Frame(LiveLifecycleFrame {
            generation: 1,
            content_count: 2,
            generation_complete: true,
            ..LiveLifecycleFrame::default()
        }),
    );
    assert_eq!(
        effects,
        vec![
            LiveLifecycleEffect::DeliverContent { count: 2 },
            LiveLifecycleEffect::FinalizeGeneration,
        ]
    );
}

#[test]
fn selected_policy_does_not_reset_backoff_on_setup_only() {
    let policy = continuous_policy();
    assert_eq!(policy.server_idle_timeout_ms, Some(15_000));
    assert_eq!(policy.server_idle_min_input_chunks, 100);
    assert_eq!(policy.rotate_after_ms, Some(720_000));
    assert_eq!(policy.rotation_quiet_ms, 3_000);
}

#[test]
fn blocking_setup_timeout_closes_before_scheduling_reconnect() {
    let mut lifecycle =
        LiveSessionLifecycle::new(continuous_policy(), LiveBackoffPolicy::default());
    assert_eq!(
        lifecycle.reduce(0, LiveLifecycleEvent::Start),
        vec![LiveLifecycleEffect::OpenSocket { generation: 1 }]
    );
    assert_eq!(
        lifecycle.reduce(100, LiveLifecycleEvent::SocketOpened { generation: 1 }),
        vec![LiveLifecycleEffect::SendSetup { generation: 1 }]
    );

    let error = anyhow::anyhow!("Gemini Live setup timed out");
    let disposition = classify_setup_failure(&error, 15_099, lifecycle.state().setup_deadline_ms);
    assert_eq!(disposition, SetupFailureDisposition::TimeoutAt(15_100));
    let SetupFailureDisposition::TimeoutAt(at_ms) = disposition else {
        unreachable!();
    };
    let effects = lifecycle.reduce(at_ms, LiveLifecycleEvent::Tick);
    assert!(matches!(
        effects.as_slice(),
        [
            LiveLifecycleEffect::CloseSocket { generation: 1 },
            LiveLifecycleEffect::ScheduleReconnect {
                generation: 2,
                attempt: 1,
                reason: LiveReconnectReason::SetupTimeout,
                ..
            }
        ]
    ));
}

#[test]
fn transient_setup_server_error_is_retryable() {
    let error = anyhow::Error::new(LiveSetupServerError {
        server: crate::api::gemini_live::ready_session::LiveServerError {
            message: "temporarily unavailable".to_string(),
            retryable: true,
        },
    });

    assert_eq!(
        classify_setup_failure(&error, 100, Some(15_000)),
        SetupFailureDisposition::ServerError { retryable: true }
    );
}
