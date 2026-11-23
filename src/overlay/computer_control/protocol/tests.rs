use super::*;

/// The production decode path with the engine's frame parser run in-process.
fn parse_with_engine(raw: &str) -> Vec<ServerEvent> {
    map_provider_events(sgt_computer_control_engine::parse_provider_frame(raw))
        .expect("engine events map to host events")
}

fn normalize(event: ServerEvent) -> Value {
    match event {
        ServerEvent::SetupComplete => json!({"type": "setup-complete"}),
        ServerEvent::Audio(samples) => {
            let bytes = samples
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>();
            json!({
                "type": "audio-pcm16",
                "dataBase64": base64::engine::general_purpose::STANDARD.encode(bytes)
            })
        }
        ServerEvent::ModelText(text) => json!({"type": "model-text", "text": text}),
        ServerEvent::Thought(text) => json!({"type": "thought", "text": text}),
        ServerEvent::InputTranscript(text) => {
            json!({"type": "input-transcript", "text": text})
        }
        ServerEvent::OutputTranscript(text) => {
            json!({"type": "output-transcript", "text": text})
        }
        ServerEvent::ToolCall { id, name, args } => {
            json!({"type": "tool-call", "id": id, "name": name, "args": args})
        }
        ServerEvent::ToolCancellation(ids) => {
            json!({"type": "tool-cancellation", "ids": ids})
        }
        ServerEvent::GenerationComplete => json!({"type": "generation-complete"}),
        ServerEvent::TurnComplete => json!({"type": "turn-complete"}),
        ServerEvent::Interrupted => json!({"type": "interrupted"}),
        ServerEvent::GoAway { time_left } => {
            json!({"type": "go-away", "timeLeft": time_left})
        }
        ServerEvent::SessionResumption { handle, resumable } => json!({
            "type": "session-resumption",
            "handle": handle,
            "resumable": resumable
        }),
        ServerEvent::Usage(metadata) => json!({"type": "usage", "metadata": metadata}),
        ServerEvent::Other(summary) => json!({"type": "other", "summary": summary}),
    }
}

#[test]
fn engine_parser_output_maps_through_the_host_for_every_shared_fixture() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/computer-control-engine/protocol-v1.json"
    )))
    .unwrap();
    for case in fixture["providerFrames"].as_array().unwrap() {
        let actual = map_provider_events(sgt_computer_control_engine::parse_provider_frame(
            &case["frame"].to_string(),
        ))
        .unwrap()
        .into_iter()
        .map(normalize)
        .collect::<Vec<_>>();
        assert_eq!(
            Value::Array(actual),
            case["expected"],
            "case {}",
            case["id"]
        );
    }
}

#[test]
fn engine_metadata_and_identifier_boundaries_survive_host_mapping() {
    for id in [
        "i".repeat(300),
        "i".repeat(MAX_PROVIDER_TOOL_ID_BYTES),
        "🌒".repeat(MAX_PROVIDER_TOOL_ID_BYTES / 4),
    ] {
        let handle = "h".repeat(MAX_PROVIDER_RESUMPTION_HANDLE_BYTES);
        let time_left = "🌒".repeat(MAX_PROVIDER_GO_AWAY_CHARS);
        let raw = json!({
            "toolCall":{"functionCalls":[{"id":id,"name":"future_tool","args":{}}]},
            "toolCallCancellation":{"ids":[id]},
            "sessionResumptionUpdate":{"newHandle":handle,"resumable":true},
            "goAway":{"timeLeft":time_left}
        })
        .to_string();
        let events =
            map_provider_events(sgt_computer_control_engine::parse_provider_frame(&raw)).unwrap();
        assert!(matches!(&events[0], ServerEvent::ToolCall { id: mapped, .. } if mapped == &id));
        assert!(matches!(&events[1], ServerEvent::ToolCancellation(ids) if ids == &[id]));
        assert!(
            matches!(&events[2], ServerEvent::GoAway { time_left: mapped } if mapped == &time_left)
        );
        assert!(matches!(&events[3], ServerEvent::SessionResumption {
            handle: Some(mapped), resumable: true
        } if mapped == &handle));
    }
}

#[test]
fn host_rejects_identifiers_and_metadata_outside_the_shared_contract() {
    for id in [
        String::new(),
        "i".repeat(MAX_PROVIDER_TOOL_ID_BYTES + 1),
        "id\0".into(),
    ] {
        assert!(
            map_provider_events(vec![ProviderEvent::ToolCall {
                id: id.clone(),
                name: "future_tool".into(),
                args: json!({})
            }])
            .is_err()
        );
        assert!(
            map_provider_events(vec![ProviderEvent::ToolCancellation { ids: vec![id] }]).is_err()
        );
    }
    for handle in [
        "h".repeat(MAX_PROVIDER_RESUMPTION_HANDLE_BYTES + 1),
        "handle\0".into(),
    ] {
        assert!(
            map_provider_events(vec![ProviderEvent::SessionResumption {
                handle: Some(handle),
                resumable: true
            }])
            .is_err()
        );
    }
    for time_left in ["t".repeat(MAX_PROVIDER_GO_AWAY_CHARS + 1), "5s\0".into()] {
        assert!(map_provider_events(vec![ProviderEvent::GoAway { time_left }]).is_err());
    }
}

#[test]
fn engine_rejects_malformed_identifiers_without_emitting_a_host_rejected_effect() {
    for id in [
        String::new(),
        "i".repeat(MAX_PROVIDER_TOOL_ID_BYTES + 1),
        "id\0".into(),
    ] {
        let raw = json!({"toolCall":{"functionCalls":[{"id":id,"name":"future_tool","args":{}}]},
            "toolCallCancellation":{"ids":[id]}})
        .to_string();
        let events =
            map_provider_events(sgt_computer_control_engine::parse_provider_frame(&raw)).unwrap();
        assert!(matches!(&events[0], ServerEvent::Other(_)));
        assert!(matches!(&events[1], ServerEvent::ToolCancellation(ids) if ids.is_empty()));
    }
}

#[test]
fn bounded_dense_engine_frames_are_not_rejected_by_smaller_host_subfield_caps() {
    let parts = vec![json!({"text":"part"}); 1100];
    let ids = (0..300)
        .map(|index| format!("call-{index}"))
        .collect::<Vec<_>>();
    let usage = json!({"metadata":"x".repeat(80 * 1024)});
    let raw = json!({"serverContent":{"modelTurn":{"parts":parts}},
        "toolCallCancellation":{"ids":ids},"usageMetadata":usage})
    .to_string();
    sgt_computer_control_protocol::Command::ParseProviderFrame { frame: raw.clone() }
        .validate()
        .unwrap();
    let events = sgt_computer_control_engine::parse_provider_frame(&raw);
    let response = sgt_computer_control_protocol::Response {
        protocol_version: sgt_computer_control_protocol::PROTOCOL_VERSION,
        token: "01".repeat(sgt_computer_control_protocol::TOKEN_BYTES),
        request_id: 1,
        output: Some(sgt_computer_control_protocol::Output::ProviderEvents {
            events: events.clone(),
        }),
        error: None,
    };
    assert!(serde_json::to_vec(&response).unwrap().len() <= MAX_JSON_BYTES);
    let mapped = map_provider_events(events).unwrap();
    assert_eq!(mapped.len(), 1102);
    assert!(matches!(&mapped[1100], ServerEvent::ToolCancellation(actual) if actual == &ids));
    assert!(matches!(&mapped[1101], ServerEvent::Usage(actual) if actual == &usage));
}

#[test]
fn parses_tool_call() {
    let raw =
        r#"{"toolCall":{"functionCalls":[{"id":"c1","name":"click","args":{"x":10,"y":20}}]}}"#;
    let evs = parse_with_engine(raw);
    match &evs[0] {
        ServerEvent::ToolCall { id, name, args } => {
            assert_eq!(id, "c1");
            assert_eq!(name, "click");
            assert_eq!(args["x"], 10);
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn server_content_yields_audio_transcript_and_turn() {
    // Two bytes: one PCM16 sample. The retired host-only parser also accepted
    // odd-length audio that the production engine parser correctly rejects.
    let raw = r#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"AQI="}}]},"outputTranscription":{"text":"ok"},"generationComplete":true,"turnComplete":true}}"#;
    let evs = parse_with_engine(raw);
    assert!(
        evs.iter()
            .any(|e| matches!(e, ServerEvent::Audio(pcm) if !pcm.is_empty()))
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, ServerEvent::OutputTranscript(t) if t == "ok"))
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, ServerEvent::GenerationComplete))
    );
    assert!(evs.iter().any(|e| matches!(e, ServerEvent::TurnComplete)));
}

#[test]
fn generation_and_turn_completion_remain_distinct_ordered_signals() {
    let events =
        parse_with_engine(r#"{"serverContent":{"generationComplete":true,"turnComplete":true}}"#);
    assert!(matches!(
        events.as_slice(),
        [ServerEvent::GenerationComplete, ServerEvent::TurnComplete]
    ));

    let generation_only =
        parse_with_engine(r#"{"serverContent":{"generationComplete":true,"turnComplete":false}}"#);
    assert!(matches!(
        generation_only.as_slice(),
        [ServerEvent::GenerationComplete]
    ));
}

#[test]
fn coalesced_tool_call_precedes_its_turn_boundary() {
    let raw = r#"{"serverContent":{"turnComplete":true},"toolCall":{"functionCalls":[{"id":"d1","name":"done","args":{"summary":"complete"}}]}}"#;
    let evs = parse_with_engine(raw);
    let call = evs
        .iter()
        .position(|event| matches!(event, ServerEvent::ToolCall { .. }))
        .unwrap();
    let boundary = evs
        .iter()
        .position(|event| matches!(event, ServerEvent::TurnComplete))
        .unwrap();
    assert!(call < boundary);
}

#[test]
fn coalesced_user_turn_precedes_model_output() {
    let raw = r#"{"serverContent":{"inputTranscription":{"text":"new goal"},"outputTranscription":{"text":"answer"}}}"#;
    let events = parse_with_engine(raw);
    let input = events
        .iter()
        .position(|event| matches!(event, ServerEvent::InputTranscript(_)))
        .unwrap();
    let output = events
        .iter()
        .position(|event| matches!(event, ServerEvent::OutputTranscript(_)))
        .unwrap();
    assert!(input < output);
}

#[test]
fn host_rejects_malformed_effect_bearing_engine_output() {
    assert!(
        map_provider_events(vec![ProviderEvent::ToolCall {
            id: "call-1".to_string(),
            name: "click".to_string(),
            args: json!([]),
        }])
        .is_err()
    );
    assert!(
        map_provider_events(vec![ProviderEvent::ToolCall {
            id: "call-1".to_string(),
            name: "invalid name".to_string(),
            args: json!({}),
        }])
        .is_err()
    );
    assert!(
        map_provider_events(vec![ProviderEvent::AudioPcm16 {
            data_base64: base64::engine::general_purpose::STANDARD.encode([1_u8]),
        }])
        .is_err()
    );
}
