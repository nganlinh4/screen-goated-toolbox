//! Setup and reconnect helpers for the Live socket.

use super::*;

const STARTUP_CATALOG_WAIT: Duration = Duration::from_secs(12);
/// DNS, TCP, TLS and WebSocket upgrade for one initial connection attempt.
const INITIAL_CONNECT_BUDGET: Duration = Duration::from_secs(15);

/// Wait only long enough to know the installed catalog's startup outcome. The
/// microphone keeps buffering on its worker while this runs; unresolved workers
/// may finish later and raise `tools_changed` for a speech-safe reconnect.
pub(super) fn await_startup_catalog(
    catalog: super::super::mcp::StartupCatalog,
    stop: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    overlay::set_status("connecting integrations...");
    let started = Instant::now();
    let report = catalog.wait(STARTUP_CATALOG_WAIT, stop);
    if report.stopped {
        return Err(anyhow::Error::new(session::SessionStopped)
            .context("stopped while activating the installed integration catalog"));
    }
    // Connections settled before this edge are represented in the initial
    // setup. Any connection that settles after it sets the flag again.
    super::super::mcp::clear_tools_changed();
    let outcome = if report.pending > 0 {
        "bounded_pending"
    } else if report.failed > 0 {
        "settled_with_failures"
    } else {
        "settled"
    };
    telemetry::event(
        "integration_catalog_startup",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "outcome": outcome,
            "installed": report.installed,
            "connected": report.connected,
            "failed": report.failed,
            "pending": report.pending,
            "wait_ms": started.elapsed().as_millis(),
        }),
    );
    overlay::push_log(format!(
        "(mcp) startup catalog: {} connected, {} failed, {} pending",
        report.connected, report.failed, report.pending
    ));
    if report.pending > 0 {
        telemetry::typed_error(
            "ERR_INTEGRATION_CATALOG_STARTUP_TIMEOUT",
            "runtime",
            "installed integration startup reached its bounded deadline; late connections remain eligible for activation",
            serde_json::json!({
                "installed": report.installed,
                "connected": report.connected,
                "failed": report.failed,
                "pending": report.pending,
                "deadline_ms": STARTUP_CATALOG_WAIT.as_millis(),
            }),
        );
    } else if report.failed > 0 {
        telemetry::typed_error(
            "ERR_INTEGRATION_CATALOG_STARTUP_FAILED",
            "runtime",
            "one or more installed integration connection attempts settled as failed",
            serde_json::json!({
                "installed": report.installed,
                "connected": report.connected,
                "failed": report.failed,
            }),
        );
    }
    Ok(())
}

pub(super) fn activation_pending(resp: &Value) -> bool {
    resp.get("activation_pending")
        .and_then(Value::as_bool)
        .or_else(|| {
            resp.pointer("/action_result/activation_pending")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

pub(super) fn configured_target() -> anyhow::Result<Option<String>> {
    std::env::var("CC_UIA_WINDOW")
        .ok()
        .map(|requested| {
            super::super::uia::stable_window_target(&requested)
                .map_err(|error| anyhow::anyhow!("cannot resolve CC_UIA_WINDOW: {error}"))
        })
        .transpose()
}

pub(super) fn connect_initial_session(key: &str, stop: &Arc<AtomicBool>) -> anyhow::Result<Sock> {
    let setup = uia_task::build_normal_setup(None)?;
    telemetry::record_model_setup(&setup, "initial_complete");
    initial_setup_with_one_retry(&setup, stop, |setup| {
        let mut socket = connect_ws(key, stop, None, Instant::now() + INITIAL_CONNECT_BUDGET)?;
        socket
            .get_mut()
            .get_mut()
            .set_write_timeout(Some(Duration::from_secs(2)))?;
        super::control::reconnect_effect(stop, || send(&mut socket, setup.clone()))?;
        wait_for_setup(&mut socket, stop)?;
        set_socket_nonblocking(&mut socket)?;
        Ok(socket)
    })
}

fn initial_setup_with_one_retry<T>(
    setup: &Value,
    stop: &AtomicBool,
    mut connect: impl FnMut(&Value) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let mut attempt = || {
        if stop.load(Ordering::SeqCst) {
            anyhow::bail!(session::SessionStopped);
        }
        let result = connect(setup);
        if stop.load(Ordering::SeqCst) {
            anyhow::bail!(session::SessionStopped);
        }
        result
    };
    match attempt() {
        Err(error)
            if !stop.load(Ordering::SeqCst)
                && !matches!(
                    session::failure_kind(&error),
                    session::SessionFailureKind::Stopped | session::SessionFailureKind::Credential
                ) =>
        {
            overlay::push_log(
                "(setup failed; retrying once with the complete catalog)".to_string(),
            );
            telemetry::event(
                "session_setup_retry",
                "runtime",
                Privacy::Sensitive,
                serde_json::json!({"catalog_policy": "complete", "search_enabled": true, "error": error.to_string()}),
            );
            attempt().map_err(|error| {
                error.context("initial setup failed twice with the complete catalog")
            })
        }
        result => result,
    }
}

fn complete_catalog_with_one_retry<T, E>(
    mut connect: impl FnMut() -> Result<T, E>,
    mut before_retry: impl FnMut(&E),
) -> Result<T, (E, E)> {
    match connect() {
        Ok(connected) => Ok(connected),
        Err(first_error) => {
            before_retry(&first_error);
            connect().map_err(|retry_error| (first_error, retry_error))
        }
    }
}

pub(super) fn activate_integrations(
    socket: &mut Sock,
    key: &str,
    target: Option<&str>,
    reconnects: &mut u32,
    state: &mut Reader,
    stop: &Arc<AtomicBool>,
) -> anyhow::Result<bool> {
    super::super::mcp::clear_tools_changed();
    overlay::push_log("(mcp) health passed - reconnecting now to activate tools".to_string());
    reconnect_session(
        socket,
        key,
        target,
        reconnects,
        state,
        stop,
        "integration_activation",
    )
}

/// Reconnect to a fresh session and restore bounded conversation context.
pub(super) fn reconnect_session(
    socket: &mut Sock,
    key: &str,
    target: Option<&str>,
    reconnects: &mut u32,
    state: &mut Reader,
    stop: &Arc<AtomicBool>,
    trigger: &str,
) -> anyhow::Result<bool> {
    super::write_recovery::retry_reseed(|| {
        reconnect_session_once(socket, key, target, reconnects, state, stop, trigger)
    })
}

fn reconnect_session_once(
    socket: &mut Sock,
    key: &str,
    target: Option<&str>,
    reconnects: &mut u32,
    state: &mut Reader,
    stop: &Arc<AtomicBool>,
    trigger: &str,
) -> anyhow::Result<bool> {
    if stop.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let started = Instant::now();
    cancel_pending_for_transport_replacement(state);
    state.connection_health.retire(reconnects, started);
    *reconnects = reconnects.saturating_add(1);
    state.reconnect_total = state.reconnect_total.saturating_add(1);
    if *reconnects > MAX_RECONNECTS {
        overlay::push_log(format!("giving up after {MAX_RECONNECTS} reconnects"));
        telemetry::typed_error(
            "ERR_SESSION_RECONNECT_LIMIT",
            "runtime",
            "session reconnect limit reached",
            serde_json::json!({"max_reconnects": MAX_RECONNECTS}),
        );
        anyhow::bail!("session reconnect limit reached after {MAX_RECONNECTS} attempts");
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
            "connection_generation": state.connection_generation,
            "trigger": trigger,
        }),
    );
    flush_reply(state);
    if !super::reconnect_gate::wait_before_attempt(*reconnects, stop) {
        return Ok(false);
    }
    let reconnect_context = reconnect_setup_context(state);
    let replacement = match complete_catalog_with_one_retry(
        || super::write_recovery::connect_replacement(key, &reconnect_context, stop),
        |first_error| {
            overlay::push_log(format!(
                "reconnect failed: {first_error} - retrying once with the complete tool catalog"
            ));
            telemetry::event(
                "session_reconnect_retry",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "attempt": *reconnects,
                    "trigger": trigger,
                    "catalog_policy": "complete",
                    "retry_limit": 1,
                }),
            );
        },
    ) {
        Ok(replacement) => replacement,
        Err((first_error, retry_error)) => {
            if stop.load(Ordering::SeqCst) {
                return Ok(false);
            }
            overlay::push_log(format!("reconnect failed again: {retry_error}"));
            telemetry::typed_error(
                "ERR_SESSION_RECONNECT_FAILED",
                "runtime",
                "session reconnect failed after one bounded retry with the complete tool catalog",
                serde_json::json!({
                    "attempt": *reconnects,
                    "catalog_policy": "complete",
                    "retry_limit": 1,
                    "first_error": first_error.to_string(),
                    "retry_error": retry_error.to_string(),
                }),
            );
            return Err(retry_error.context(
                "session reconnect failed after one retry with the complete tool catalog",
            ));
        }
    };
    if stop.load(Ordering::SeqCst) {
        return Ok(false);
    }
    *socket = replacement;
    state.connection_health.connected(Instant::now());
    state.connection_generation = state.connection_generation.saturating_add(1);
    let terminal_response_open = state.terminal_response.is_open();
    if super::terminal_drain::retire_for_connection_replacement(state) {
        telemetry::event(
            "closed_generation_latch_discarded",
            "runtime",
            Privacy::Safe,
            serde_json::json!({
                "reason": "connection_replaced",
                "trigger": trigger,
                "connection_generation": state.connection_generation,
                "final_response_was_open": terminal_response_open,
            }),
        );
    }
    state.ignore_stale_boundary = false;
    state.nudged = false;
    state.source_frame = None;
    super::speech_events::discard_generation_audio(state, "connection_replaced");
    if stop.load(Ordering::SeqCst) {
        return Ok(false);
    }
    match uia_task::snapshot(target) {
        Ok(frame) => {
            super::control::reconnect_effect(stop, || {
                super::frames::send_snapshot(socket, &frame, "reconnect_reseed")
            })?;
            state
                .memory
                .snapshot(&frame, state.active && !state.terminal_drain, false);
            state.source_frame = Some(frame.source);
        }
        Err(error) => super::frames::capture_failed("reconnect_reseed", target, &error),
    }
    if stop.load(Ordering::SeqCst) {
        return Ok(false);
    }
    if !state.active {
        state.awaiting = false;
        state.recovery_owed = false;
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
                "history_chars": reconnect_context.chars().count(),
                "history_entries": state.history.len(),
            }),
        );
        return Ok(true);
    }
    let recap = active_reconnect_context(state);
    let msg = format!(
        "(transport reconnected; this is not a new user request)\n{recap}\nContinue only the committed task from current tool and screen evidence. Never infer task completion from receipt or delivery status alone."
    );
    super::control::reconnect_effect(stop, || send(socket, realtime_text(&msg)))?;
    state.awaiting = true;
    state.recovery_owed = true;
    state.think_start = Some(Instant::now());
    overlay::push_log("(reconnected - conversation memory restored)".to_string());
    overlay::set_status("ready - speak a command");
    telemetry::event(
        "session_reconnect_reseeded",
        "runtime",
        Privacy::Safe,
        serde_json::json!({
            "attempt": *reconnects,
            "recap_chars": recap.chars().count(),
            "history_chars": reconnect_context.chars().count(),
            "history_entries": state.history.len(),
            "turn_mode": state.turn_mode.as_str(),
            "connection_generation": state.connection_generation,
            "trigger": trigger,
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
    Ok(true)
}

fn reconnect_setup_context(state: &Reader) -> String {
    const BUDGET: usize = 6000;
    const ENTRY_BUDGET: usize = 2000;
    let mut remaining = BUDGET;
    let mut recent = Vec::new();
    for entry in state.history.iter().rev() {
        if remaining == 0 {
            break;
        }
        let clipped = entry
            .chars()
            .take(remaining.min(ENTRY_BUDGET))
            .collect::<String>();
        remaining = remaining.saturating_sub(clipped.chars().count() + 1);
        recent.push(clipped);
    }
    recent.reverse();
    recent.join("\n")
}

fn active_reconnect_context(state: &Reader) -> String {
    const BUDGET: usize = 1000;
    const OUTCOME_BUDGET: usize = 220;
    let goal: String = state.last_cmd.trim().chars().take(500).collect();
    let mut parts = vec![format!("Committed task: {goal}")];
    if state.reconciliation.required() {
        parts.push(if state.reconciliation.unresolved() {
            "An earlier action may have taken effect. Automatic reconciliation is unavailable in this session; the user must inspect affected state before starting another session.".to_string()
        } else {
            "An interrupted action is still settling; do not retry or claim completion before its terminal receipt.".to_string()
        });
    }
    let outcomes = state.turn_outcomes.reconnect_summary(OUTCOME_BUDGET);
    if !outcomes.is_empty() {
        parts.push(format!(
            "Delivered tool result states (not task-completion claims): {outcomes}"
        ));
    }
    let joined = parts.join("\n");
    joined.chars().take(BUDGET).collect()
}

fn cancel_pending_for_transport_replacement(state: &mut Reader) -> bool {
    // Tool call IDs belong to the old connection, even when no action is pending.
    state.immediate_tool_responses.clear();
    let first_cancel_request = state.pending.id.is_some() && !state.pending.cancelled;
    let pending_tool = state.pending.tool.clone();
    let cancelled = state.pending.request_cancel();
    if cancelled && let Some(pending_tool) = pending_tool {
        let mutating = super::super::turn_policy::is_mutating_tool(&pending_tool);
        let newly_reconciling = mutating && !state.reconciliation.required();
        if mutating && state.reconciliation.require_pending(&state.pending) {
            super::reconciliation::warn_unresolved();
        }
        if first_cancel_request || newly_reconciling {
            state.turn_outcomes.record_transport_interruption_for(
                &pending_tool,
                super::reconciliation::JobIdentity::from_pending(&state.pending),
            );
            telemetry::event(
                "pending_action_transport_interrupted",
                "runtime",
                Privacy::Safe,
                serde_json::json!({
                    "tool": pending_tool,
                    "effect": "unknown",
                    "cancel_requested": true,
                }),
            );
        }
    }
    cancelled
}

#[cfg(test)]
#[path = "response_cancellation_tests.rs"]
mod response_cancellation_tests;

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
        if session::setup_frame_ready(&text)? {
            return Ok(());
        }
    }
    anyhow::bail!(session::SessionStopped)
}

#[cfg(test)]
#[path = "session_control_tests.rs"]
mod tests;
