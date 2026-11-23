use super::*;
use anyhow::anyhow;
use sgt_computer_control_protocol::ProviderEvent;

fn fixture() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/provider-frame-recovery.json"
    )))
    .unwrap()
}

fn rejection(kind: RejectionKind, message: &str) -> anyhow::Error {
    match kind {
        RejectionKind::Request => DataRejection::request(message).into(),
        RejectionKind::Application => DataRejection::application(message).into(),
        RejectionKind::ProviderFrame => DataRejection::provider_frame(message).into(),
    }
}

#[test]
fn shared_decoder_cases_recover_only_typed_data_rejections() {
    for case in fixture()["windowsDecoderCases"].as_array().unwrap() {
        let outcome = case["outcome"].as_str().unwrap();
        let error = match outcome {
            "request-rejected" => rejection(RejectionKind::Request, outcome),
            "application-rejected" => rejection(RejectionKind::Application, outcome),
            "provider-data-rejected" => rejection(RejectionKind::ProviderFrame, outcome),
            _ => anyhow!(outcome.to_owned()),
        };
        let mut input = ProviderInput::default();
        let mut notices = Vec::new();
        let result = input.decode_with(
            "untrusted input",
            |_| Err(error),
            |kind, bytes| notices.push((kind, bytes)),
        );
        if case["expected"] == "fatal" {
            assert!(result.is_err(), "{outcome}");
            assert!(notices.is_empty());
            assert!(input.summary().is_none());
        } else {
            assert!(result.unwrap().is_none(), "{outcome}");
            assert_eq!(notices.len(), 1);
            let next = input
                .decode_with(
                    "next",
                    |_| Ok(vec![ServerEvent::InputTranscript("next".into())]),
                    |_, _| panic!("valid frame rejected"),
                )
                .unwrap()
                .unwrap();
            assert!(
                matches!(next.as_slice(), [ServerEvent::InputTranscript(text)] if text == "next")
            );
        }
    }
}

#[test]
fn rejected_decoded_batch_cannot_publish_a_valid_prefix_or_completion() {
    let mut input = ProviderInput::default();
    let rejected = input
        .decode_with(
            "batch",
            |_| {
                protocol::decode_provider_events(vec![
                    ProviderEvent::ToolCall {
                        id: "call".into(),
                        name: "future_tool".into(),
                        args: json!({}),
                    },
                    ProviderEvent::TurnComplete,
                    ProviderEvent::AudioPcm16 {
                        data_base64: "AQ==".into(),
                    },
                ])
            },
            |kind, _| assert_eq!(kind, RejectionKind::ProviderFrame),
        )
        .unwrap();
    assert!(rejected.is_none());
    let next = input
        .decode_with(
            "valid batch",
            |_| {
                protocol::decode_provider_events(vec![
                    ProviderEvent::ToolCall {
                        id: "following".into(),
                        name: "future_tool".into(),
                        args: json!({}),
                    },
                    ProviderEvent::TurnComplete,
                ])
            },
            |_, _| panic!("valid batch rejected"),
        )
        .unwrap()
        .unwrap();
    assert!(
        matches!(next.as_slice(), [ServerEvent::ToolCall { id, .. }, ServerEvent::TurnComplete] if id == "following")
    );
}

#[test]
fn contextual_types_survive_but_error_wording_cannot_authorize_recovery() {
    let mut input = ProviderInput::default();
    let error = rejection(RejectionKind::Application, "detail").context("outer context");
    assert!(
        input
            .decode_with("frame", |_| Err(error), |_, _| {})
            .unwrap()
            .is_none()
    );
    let error = anyhow!("provider data was rejected before effects")
        .context("application request rejection");
    assert!(
        input
            .decode_with(
                "frame",
                |_| Err(error),
                |_, _| panic!("untyped error was downgraded")
            )
            .is_err()
    );
}

#[test]
fn rejection_diagnostics_are_bounded_and_contain_only_kinds_and_counts() {
    let mut input = ProviderInput::default();
    let raw = "private frame \u{1f30f}";
    let mut notices = Vec::new();
    for index in 0..1024 {
        let kind = RejectionKind::ALL[index % 3];
        let _ = input.decode_with(
            raw,
            |_| Err(rejection(kind, "private error detail")),
            |kind, bytes| notices.push((kind, bytes)),
        );
    }
    assert_eq!(notices.len(), 3);
    assert!(notices.iter().all(|(_, bytes)| *bytes == raw.len()));
    let summary = input.summary().unwrap();
    let records = summary["rejected"].as_array().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(
        records
            .iter()
            .map(|record| record["frames"].as_u64().unwrap())
            .sum::<u64>(),
        1024
    );
    assert_eq!(
        records
            .iter()
            .map(|record| record["bytes"].as_u64().unwrap())
            .sum::<u64>(),
        1024 * raw.len() as u64
    );
    let encoded = summary.to_string();
    assert!(encoded.len() < 500);
    assert!(!encoded.contains("private"));
    assert!(!encoded.contains("detail"));
}

#[test]
fn rejection_counts_saturate_without_reopening_first_notice() {
    let mut input = ProviderInput::default();
    input.rejected[RejectionKind::Request as usize] = RejectedFrames {
        frames: u64::MAX - 1,
        bytes: u64::MAX - 1,
    };
    for _ in 0..2 {
        let _ = input.decode_with(
            "bytes",
            |_| Err(rejection(RejectionKind::Request, "detail")),
            |_, _| panic!("existing kind was reported twice"),
        );
    }
    let counts = input.rejected[RejectionKind::Request as usize];
    assert_eq!(counts.frames, u64::MAX);
    assert_eq!(counts.bytes, u64::MAX);
}

#[test]
fn valid_event_order_and_empty_frames_remain_distinct_from_rejection() {
    let mut input = ProviderInput::default();
    let events = input
        .decode_with(
            "valid",
            |_| {
                protocol::decode_provider_events(vec![
                    ProviderEvent::InputTranscript {
                        text: "goal".into(),
                    },
                    ProviderEvent::ToolCall {
                        id: "call".into(),
                        name: "future_tool".into(),
                        args: json!({}),
                    },
                    ProviderEvent::GenerationComplete,
                    ProviderEvent::TurnComplete,
                ])
            },
            |_, _| panic!("valid input rejected"),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        events.as_slice(),
        [
            ServerEvent::InputTranscript(_),
            ServerEvent::ToolCall { .. },
            ServerEvent::GenerationComplete,
            ServerEvent::TurnComplete
        ]
    ));
    assert!(
        input
            .decode_with(
                "empty",
                |_| Ok(Vec::new()),
                |_, _| panic!("empty input rejected")
            )
            .unwrap()
            .unwrap()
            .is_empty()
    );
    assert!(input.summary().is_none());
}

#[test]
fn shared_raw_sequences_preserve_following_content_without_false_progress() {
    for case in fixture()["wireSequences"].as_array().unwrap() {
        let mut input = ProviderInput::default();
        let parse = |raw: &str| {
            protocol::decode_provider_events(sgt_computer_control_engine::parse_provider_frame(raw))
        };
        let ignored = input
            .decode_with(case["rejected"].as_str().unwrap(), parse, |_, _| {})
            .unwrap()
            .unwrap();
        assert!(
            !ignored
                .iter()
                .any(super::super::reconnect_gate::generation_progress)
        );
        assert!(
            !ignored
                .iter()
                .any(|event| matches!(event, ServerEvent::ToolCall { .. }))
        );
        let events = input
            .decode_with(case["next"].as_str().unwrap(), parse, |_, _| {
                panic!("following input rejected")
            })
            .unwrap()
            .unwrap();
        assert!(
            matches!(events.as_slice(), [ServerEvent::InputTranscript(text)] if text == case["expectedInput"].as_str().unwrap())
        );
    }
}
