use serde_json::{Value, json};

use super::lifecycle::*;

const FIXTURE: &str = include_str!("../../../parity-fixtures/gemini-live-session/lifecycle.json");

#[test]
fn lifecycle_matches_shared_parity_fixture() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let backoff = backoff_policy(&fixture["backoffFormula"]);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let profile_name = case["profile"].as_str().unwrap();
        let policy = lifecycle_policy(&fixture["profiles"][profile_name]);
        let mut lifecycle = LiveSessionLifecycle::new(policy, backoff);
        if let Some(arrangement) = case.get("arrange").and_then(Value::as_str) {
            replay_steps(
                &mut lifecycle,
                fixture["arrangements"][arrangement].as_array().unwrap(),
                name,
                false,
            );
        }
        replay_steps(
            &mut lifecycle,
            case["steps"].as_array().unwrap(),
            name,
            true,
        );
    }
}

fn replay_steps(
    lifecycle: &mut LiveSessionLifecycle,
    steps: &[Value],
    case_name: &str,
    assert_expected: bool,
) {
    for step in steps {
        let at_ms = step["atMs"].as_u64().unwrap();
        let effects = lifecycle.reduce(at_ms, event(&step["event"]));
        if !assert_expected {
            continue;
        }
        assert_state(lifecycle.state(), &step["expectState"], case_name);
        assert_eq!(
            effects_json(&effects),
            step["expectEffects"],
            "effects mismatch in {case_name} at {at_ms}ms"
        );
    }
}

fn event(value: &Value) -> LiveLifecycleEvent {
    match value["type"].as_str().unwrap() {
        "start" => LiveLifecycleEvent::Start,
        "socketOpened" => LiveLifecycleEvent::SocketOpened {
            generation: value["generation"].as_u64().unwrap(),
        },
        "frame" => LiveLifecycleEvent::Frame(LiveLifecycleFrame {
            generation: value["generation"].as_u64().unwrap(),
            content_count: value
                .get("contentCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            setup_complete: bool_field(value, "setupComplete"),
            turn_complete: bool_field(value, "turnComplete"),
            generation_complete: bool_field(value, "generationComplete"),
            interrupted: bool_field(value, "interrupted"),
            go_away_time_left_ms: value.get("goAwayTimeLeftMs").and_then(Value::as_u64),
            tool_call_ids: string_array(value.get("toolCallIds")),
            tool_cancellation_ids: string_array(value.get("toolCancellationIds")),
            error: value.get("error").map(|error| LiveClassifiedError {
                kind: error["kind"].as_str().unwrap().to_string(),
                retryable: error["retryable"].as_bool().unwrap(),
            }),
        }),
        "transportFailure" => LiveLifecycleEvent::TransportFailure {
            generation: value["generation"].as_u64().unwrap(),
            retryable: value["retryable"].as_bool().unwrap(),
        },
        "inputSent" => LiveLifecycleEvent::InputSent {
            chunks: value["chunks"].as_u64().unwrap(),
        },
        "inputActivity" => LiveLifecycleEvent::InputActivity,
        "workState" => LiveLifecycleEvent::WorkState {
            pending_work_count: value["pendingWorkCount"].as_u64().unwrap(),
            buffered_input_count: value["bufferedInputCount"].as_u64().unwrap(),
            user_speaking: value["userSpeaking"].as_bool().unwrap(),
        },
        "tick" => LiveLifecycleEvent::Tick,
        "cancel" => LiveLifecycleEvent::Cancel,
        other => panic!("unknown lifecycle event {other}"),
    }
}

fn assert_state(state: &LiveLifecycleState, expected: &Value, case_name: &str) {
    let actual = state_json(state);
    for (field, expected_value) in expected.as_object().unwrap() {
        assert_eq!(
            actual.get(field),
            Some(expected_value),
            "state field {field} mismatch in {case_name}"
        );
    }
}

fn state_json(state: &LiveLifecycleState) -> Value {
    json!({
        "phase": phase_name(state.phase),
        "generation": state.generation,
        "setupDeadlineMs": state.setup_deadline_ms,
        "firstResponseDeadlineMs": state.first_response_deadline_ms,
        "contentIdleDeadlineMs": state.content_idle_deadline_ms,
        "hardResponseDeadlineMs": state.hard_response_deadline_ms,
        "reconnectDeadlineMs": state.reconnect_deadline_ms,
        "goAwayDeadlineMs": state.go_away_deadline_ms,
        "reconnectAttempt": state.reconnect_attempt,
        "hasOutput": state.has_output,
        "inputChunksSinceServerActivity": state.input_chunks_since_server_activity,
        "lastInputActivityMs": state.last_input_activity_ms,
        "pendingWorkCount": state.pending_work_count,
        "bufferedInputCount": state.buffered_input_count,
        "userSpeaking": state.user_speaking,
        "pendingToolIds": state.pending_tool_ids,
    })
}

fn effects_json(effects: &[LiveLifecycleEffect]) -> Value {
    Value::Array(
        effects
            .iter()
            .map(|effect| match effect {
                LiveLifecycleEffect::OpenSocket { generation } => {
                    json!({"type":"openSocket","generation":generation})
                }
                LiveLifecycleEffect::SendSetup { generation } => {
                    json!({"type":"sendSetup","generation":generation})
                }
                LiveLifecycleEffect::DeliverContent { count } => {
                    json!({"type":"deliverContent","count":count})
                }
                LiveLifecycleEffect::FinalizeResponse { reason } => {
                    json!({"type":"finalizeResponse","reason":completion_name(*reason)})
                }
                LiveLifecycleEffect::FinalizeGeneration => json!({"type":"finalizeGeneration"}),
                LiveLifecycleEffect::FinalizeTurn => json!({"type":"finalizeTurn"}),
                LiveLifecycleEffect::StopPlayback => json!({"type":"stopPlayback"}),
                LiveLifecycleEffect::DiscardQueuedOutput => json!({"type":"discardQueuedOutput"}),
                LiveLifecycleEffect::FinalizeInterruptedGeneration => {
                    json!({"type":"finalizeInterruptedGeneration"})
                }
                LiveLifecycleEffect::DispatchTools { ids } => {
                    json!({"type":"dispatchTools","ids":ids})
                }
                LiveLifecycleEffect::CancelTools { ids } => {
                    json!({"type":"cancelTools","ids":ids})
                }
                LiveLifecycleEffect::CloseSocket { generation } => {
                    json!({"type":"closeSocket","generation":generation})
                }
                LiveLifecycleEffect::ScheduleReconnect {
                    generation,
                    attempt,
                    delay_ms,
                    reason,
                } => json!({
                    "type":"scheduleReconnect",
                    "generation":generation,
                    "attempt":attempt,
                    "delayMs":delay_ms,
                    "reason":reconnect_name(*reason),
                }),
                LiveLifecycleEffect::ReportFailure { reason } => {
                    json!({"type":"reportFailure","reason":reason})
                }
                LiveLifecycleEffect::CancelSession => json!({"type":"cancelSession"}),
            })
            .collect(),
    )
}

fn lifecycle_policy(value: &Value) -> LiveLifecyclePolicy {
    let kind = match value["kind"].as_str().unwrap() {
        "finiteRequest" => LiveSessionKind::FiniteRequest,
        "continuousStream" => LiveSessionKind::ContinuousStream,
        "agentSession" => LiveSessionKind::AgentSession,
        other => panic!("unknown session kind {other}"),
    };
    let signals = string_array(value.get("finiteCompletionSignals"));
    LiveLifecyclePolicy {
        kind,
        setup_timeout_ms: value["setupTimeoutMs"].as_u64().unwrap(),
        first_response_timeout_ms: optional_u64(value, "firstResponseTimeoutMs"),
        content_idle_ms: optional_u64(value, "contentIdleMs"),
        hard_response_timeout_ms: optional_u64(value, "hardResponseTimeoutMs"),
        server_idle_timeout_ms: optional_u64(value, "serverIdleTimeoutMs"),
        server_idle_min_input_chunks: optional_u64(value, "serverIdleMinInputChunks").unwrap_or(0),
        rotate_after_ms: optional_u64(value, "rotateAfterMs"),
        rotation_quiet_ms: optional_u64(value, "rotationQuietMs").unwrap_or(0),
        reconnect_enabled: value["reconnectEnabled"].as_bool().unwrap(),
        max_reconnect_attempts: optional_u64(value, "maxReconnectAttempts").map(|v| v as u32),
        complete_on_turn: kind != LiveSessionKind::FiniteRequest
            || signals.iter().any(|signal| signal == "turnComplete"),
        complete_on_generation: kind != LiveSessionKind::FiniteRequest
            || signals.iter().any(|signal| signal == "generationComplete"),
    }
}

fn backoff_policy(value: &Value) -> LiveBackoffPolicy {
    LiveBackoffPolicy {
        base_ms: value["baseMs"].as_u64().unwrap(),
        exponent_cap: value["exponentCap"].as_u64().unwrap() as u32,
        jitter_min_ms: value["jitterMinMs"].as_u64().unwrap(),
        jitter_seed: value["jitterSeed"].as_u64().unwrap(),
        jitter_step: value["jitterStep"].as_u64().unwrap(),
        jitter_span: value["jitterSpan"].as_u64().unwrap(),
        max_ms: value["maxMs"].as_u64().unwrap(),
    }
}

fn optional_u64(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

fn bool_field(value: &Value, field: &str) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

fn phase_name(phase: LiveSessionPhase) -> &'static str {
    match phase {
        LiveSessionPhase::Idle => "idle",
        LiveSessionPhase::Connecting => "connecting",
        LiveSessionPhase::AwaitingSetup => "awaitingSetup",
        LiveSessionPhase::Active => "active",
        LiveSessionPhase::BackingOff => "backingOff",
        LiveSessionPhase::Completed => "completed",
        LiveSessionPhase::Cancelled => "cancelled",
        LiveSessionPhase::Failed => "failed",
    }
}

fn completion_name(reason: LiveCompletionReason) -> &'static str {
    match reason {
        LiveCompletionReason::TurnComplete => "turnComplete",
        LiveCompletionReason::GenerationComplete => "generationComplete",
        LiveCompletionReason::ContentIdle => "contentIdle",
    }
}

fn reconnect_name(reason: LiveReconnectReason) -> &'static str {
    match reason {
        LiveReconnectReason::SetupTimeout => "setupTimeout",
        LiveReconnectReason::TransportFailure => "transportFailure",
        LiveReconnectReason::ServerError => "serverError",
        LiveReconnectReason::ServerIdle => "serverIdle",
        LiveReconnectReason::ProactiveRotation => "proactiveRotation",
        LiveReconnectReason::GoAwaySafeGap => "goAwaySafeGap",
        LiveReconnectReason::GoAwayDeadline => "goAwayDeadline",
    }
}
