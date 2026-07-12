//! Setup and reconnect helpers for the Live socket.

use super::*;

pub(super) fn activation_pending(resp: &Value) -> bool {
    resp.get("activation_pending")
        .and_then(Value::as_bool)
        .or_else(|| {
            resp.pointer("/action_result/activation_pending")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// Reconnect to a fresh session and restore bounded conversation context.
pub(super) fn reconnect_session(
    socket: &mut Sock,
    key: &str,
    target: Option<&str>,
    reconnects: &mut u32,
    state: &mut Reader,
    trigger: &str,
) -> anyhow::Result<bool> {
    let started = Instant::now();
    *reconnects += 1;
    state.reconnect_total = state.reconnect_total.saturating_add(1);
    if *reconnects > MAX_RECONNECTS {
        overlay::push_log(format!("giving up after {MAX_RECONNECTS} reconnects"));
        telemetry::typed_error(
            "ERR_SESSION_RECONNECT_LIMIT",
            "runtime",
            "session reconnect limit reached",
            serde_json::json!({"max_reconnects": MAX_RECONNECTS}),
        );
        return Ok(false);
    }
    overlay::set_status("reconnecting...");
    telemetry::event(
        "session_reconnect_start",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "attempt": *reconnects,
            "active": state.active,
            "awaiting": state.awaiting,
            "turn_mode": state.turn_mode.as_str(),
            "control_revoked": state.control_revoked,
            "connection_generation": state.connection_generation,
            "trigger": trigger,
        }),
    );
    match uia_task::reconnect(key, None, true, false) {
        Ok(s) => *socket = s,
        Err(e) => {
            overlay::push_log(format!(
                "reconnect failed: {e} - retrying without MCP tools"
            ));
            super::super::mcp::set_suppress_tools(true);
            match uia_task::reconnect(key, None, true, false) {
                Ok(s) => *socket = s,
                Err(e2) => {
                    overlay::push_log(format!("reconnect failed again: {e2}"));
                    telemetry::typed_error(
                        "ERR_SESSION_RECONNECT_FAILED",
                        "runtime",
                        "session reconnect failed with and without optional tools",
                        serde_json::json!({
                            "attempt": *reconnects,
                            "first_error": e.to_string(),
                            "fallback_error": e2.to_string(),
                        }),
                    );
                    return Ok(false);
                }
            }
        }
    }
    state.connection_generation = state.connection_generation.saturating_add(1);
    state.pending.request_cancel();
    state.pending = Pending::default();
    if state.awaiting_done_boundary {
        state.awaiting_done_boundary = false;
        telemetry::event(
            "done_turn_boundary_discarded",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "reason": "connection_replaced",
                "trigger": trigger,
                "connection_generation": state.connection_generation,
            }),
        );
    }
    state.nudged = false;
    flush_reply(state);
    match uia_task::snapshot(target) {
        Ok(frame) => {
            super::frames::send_snapshot(socket, &frame, "reconnect_reseed")?;
            state.source_frame_id = Some(frame.frame_id);
        }
        Err(error) => super::frames::capture_failed("reconnect_reseed", target, &error),
    }
    if !state.active {
        overlay::push_log("(reconnected - idle; waiting for user)".to_string());
        overlay::set_status("ready - speak a command");
        telemetry::event(
            "session_reconnect_idle",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "attempt": *reconnects,
                "trigger": trigger,
                "duration_ms": started.elapsed().as_millis(),
                "connection_generation": state.connection_generation,
            }),
        );
        return Ok(true);
    }
    let recap = build_recap(&state.history);
    let judge = "JUDGE before doing anything: only finish a step if you were CLEARLY mid-way through an action the \
user already asked for AND the current screen is that task. Otherwise - task looks done, screen is unrelated, or \
you're unsure - take NO action and simply wait for the user (no narration needed). NEVER start a new or consequential \
action just because the connection reconnected.";
    let msg = if recap.is_empty() {
        format!("(reconnected seamlessly - not a new request) The current screen is shown. {judge}")
    } else {
        format!(
            "(reconnected seamlessly - not a new request) Our conversation so far, keep it as context:\n{recap}\n\nThe \
current screen is shown. {judge}"
        )
    };
    send(socket, realtime_text(&msg))?;
    overlay::push_log("(reconnected - conversation memory restored)".to_string());
    overlay::set_status("ready - speak a command");
    telemetry::event(
        "session_reconnect_reseeded",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "attempt": *reconnects,
            "recap_chars": recap.chars().count(),
            "turn_mode": state.turn_mode.as_str(),
            "control_revoked": state.control_revoked,
            "connection_generation": state.connection_generation,
            "trigger": trigger,
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
    Ok(true)
}

pub(super) fn record_session_end(state: &Reader, reason: &str) {
    telemetry::event(
        "session_end",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "reason": reason,
            "connection_generation": state.connection_generation,
            "reconnect_total": state.reconnect_total,
            "turn_mode": state.turn_mode.as_str(),
            "control_revoked": state.control_revoked,
            "pending_tool": state.pending.id.clone(),
            "history_entries": state.history.len(),
            "runtime_cleanup_complete": true,
        }),
    );
}

pub(super) fn foreground_is_browser() -> bool {
    let title = super::super::uia::pointer_context().0.to_lowercase();
    [
        "chrome", "edge", "brave", "opera", "firefox", "chromium", "vivaldi",
    ]
    .iter()
    .any(|browser| title.contains(browser))
}

pub(super) fn wait_for_setup(socket: &mut Sock, stop: &Arc<AtomicBool>) -> anyhow::Result<()> {
    set_socket_short_timeout(socket)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !stop.load(Ordering::SeqCst) {
        if Instant::now() > deadline {
            anyhow::bail!("timed out waiting for setupComplete");
        }
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(frame)) => anyhow::bail!("server closed during setup: {frame:?}"),
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => anyhow::bail!("setup read error: {e}"),
        };
        if parse_server_message(&text)
            .into_iter()
            .any(|ev| matches!(ev, ServerEvent::SetupComplete))
        {
            return Ok(());
        }
    }
    anyhow::bail!("stopped")
}
