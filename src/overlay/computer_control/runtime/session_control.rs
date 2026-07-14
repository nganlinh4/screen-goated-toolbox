//! Setup and reconnect helpers for the Live socket.

use super::*;

const STARTUP_CATALOG_WAIT: Duration = Duration::from_secs(12);
const CATALOG_RETRY_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(super) struct CatalogRecovery {
    retry_at: Option<Instant>,
    retry_available: bool,
    terminal_reported: bool,
}

impl Default for CatalogRecovery {
    fn default() -> Self {
        Self {
            retry_at: None,
            retry_available: true,
            terminal_reported: false,
        }
    }
}

impl CatalogRecovery {
    fn catalog_present(&mut self) {
        self.retry_at = None;
        self.retry_available = true;
        self.terminal_reported = false;
    }

    fn catalog_omitted(&mut self, now: Instant) {
        if self.retry_at.is_some() {
            return;
        }
        if self.retry_available {
            self.retry_at = Some(now + CATALOG_RETRY_BACKOFF);
            overlay::push_log(format!(
                "(mcp) fallback catalog retry queued for an idle gap in {}s",
                CATALOG_RETRY_BACKOFF.as_secs()
            ));
            telemetry::event(
                "integration_catalog_retry_scheduled",
                "runtime",
                Privacy::Safe,
                serde_json::json!({"backoff_ms": CATALOG_RETRY_BACKOFF.as_millis()}),
            );
        } else if !self.terminal_reported {
            self.terminal_reported = true;
            telemetry::typed_error(
                "ERR_INTEGRATION_CATALOG_REACTIVATION_EXHAUSTED",
                "runtime",
                "the bounded full-catalog retry failed; the current connection remains usable without integration proxies",
                serde_json::json!({"retry_limit": 1}),
            );
        }
    }

    pub(super) fn retry_due(&self, now: Instant) -> bool {
        self.retry_at.is_some_and(|retry_at| now >= retry_at)
    }

    pub(super) fn begin_retry(&mut self) {
        self.retry_at = None;
        self.retry_available = false;
    }
}

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
        anyhow::bail!("stopped while activating the installed integration catalog");
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
    let mut socket = connect_ws(key)?;
    let setup_with_search = uia_task::build_setup(None, true, true);
    telemetry::record_model_setup(&setup_with_search, "initial_search");
    send(&mut socket, setup_with_search)?;
    if wait_for_setup(&mut socket, stop).is_err() {
        let _ = socket.close(None);
        overlay::push_log(
            "(Google Search unavailable on this key; starting without it)".to_string(),
        );
        socket = connect_ws(key)?;
        let setup_without_search = uia_task::build_setup(None, true, false);
        telemetry::record_model_setup(&setup_without_search, "initial_fallback");
        send(&mut socket, setup_without_search)?;
        wait_for_setup(&mut socket, stop)?;
    }
    set_socket_nonblocking(&mut socket)?;
    Ok(socket)
}

pub(super) fn activate_integrations(
    socket: &mut Sock,
    key: &str,
    target: Option<&str>,
    reconnects: &mut u32,
    state: &mut Reader,
    catalog_recovery: &mut CatalogRecovery,
) -> anyhow::Result<bool> {
    super::super::mcp::clear_tools_changed();
    overlay::push_log("(mcp) health passed - reconnecting now to activate tools".to_string());
    reconnect_session(
        socket,
        key,
        target,
        reconnects,
        state,
        catalog_recovery,
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
    catalog_recovery: &mut CatalogRecovery,
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
            "connection_generation": state.connection_generation,
            "trigger": trigger,
        }),
    );
    cancel_pending_for_transport_replacement(state);
    let catalog_omitted = match uia_task::reconnect(key, None, true, false, true) {
        Ok(s) => {
            *socket = s;
            false
        }
        Err(e) => {
            overlay::push_log(format!(
                "reconnect failed: {e} - retrying without MCP tools"
            ));
            match uia_task::reconnect(key, None, true, false, false) {
                Ok(s) => {
                    *socket = s;
                    true
                }
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
    };
    if catalog_omitted {
        catalog_recovery.catalog_omitted(Instant::now());
    } else {
        catalog_recovery.catalog_present();
    }
    state.connection_generation = state.connection_generation.saturating_add(1);
    state.pending = Pending::default();
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
    flush_reply(state);
    match uia_task::snapshot(target) {
        Ok(frame) => {
            super::frames::send_snapshot(socket, &frame, "reconnect_reseed")?;
            state.source_frame = Some(frame.source);
        }
        Err(error) => super::frames::capture_failed("reconnect_reseed", target, &error),
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
            }),
        );
        return Ok(true);
    }
    let recap = active_reconnect_context(state);
    let msg = format!(
        "(transport reconnected; this is not a new user request)\n{recap}\nContinue only the committed task from current tool and screen evidence. Never infer task completion from receipt or delivery status alone."
    );
    send(socket, realtime_text(&msg))?;
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
            "turn_mode": state.turn_mode.as_str(),
            "connection_generation": state.connection_generation,
            "trigger": trigger,
            "duration_ms": started.elapsed().as_millis(),
        }),
    );
    Ok(true)
}

fn active_reconnect_context(state: &Reader) -> String {
    const BUDGET: usize = 1000;
    const OUTCOME_BUDGET: usize = 220;
    let goal: String = state.last_cmd.trim().chars().take(500).collect();
    let mut parts = vec![format!("Committed task: {goal}")];
    if state.turn_outcomes.has_transport_uncertainty() {
        parts.push(
            "A transport-replaced action has an unknown effect; inspect current state before deciding whether any retry is safe."
                .to_string(),
        );
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
    let pending_tool = state.pending.tool.take();
    let cancelled = state.pending.request_cancel();
    if cancelled && let Some(pending_tool) = pending_tool {
        state
            .turn_outcomes
            .record_transport_interruption(&pending_tool);
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
    cancelled
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_context_keeps_only_goal_and_structural_outcomes() {
        let mut state = Reader {
            last_cmd: "current committed task".into(),
            history: vec![
                "User: old task".into(),
                "Assistant: misleading old narration".into(),
                "User: current committed task".into(),
                "Observed (future_reader): current evidence".into(),
                "Assistant: untrusted draft".into(),
            ],
            turn_tools: vec!["received_but_not_delivered".into()],
            ..Reader::default()
        };
        state.turn_outcomes.record_delivered(
            "delivered_reader",
            &serde_json::json!({"ok": true, "content": "sensitive-result"}),
        );
        state.turn_outcomes.record_delivered(
            "failed_writer",
            &serde_json::json!({"ok": false, "error": "sensitive-error"}),
        );
        let context = active_reconnect_context(&state);
        assert!(context.contains("current committed task"));
        assert!(context.contains("delivered_reader=delivered_ok"));
        assert!(context.contains("failed_writer=delivered_failed"));
        assert!(!context.contains("received_but_not_delivered"));
        assert!(!context.contains("sensitive-result"));
        assert!(!context.contains("sensitive-error"));
        assert!(!context.contains("current evidence"));
        assert!(!context.contains("old task"));
        assert!(!context.contains("narration"));
        assert!(!context.contains("untrusted draft"));
    }

    #[test]
    fn transport_replacement_records_uncertainty_and_cancels_pending_work() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut state = Reader {
            pending: Pending {
                id: Some("pending-id".to_string()),
                tool: Some("future_operation".to_string()),
                cancelled: false,
                cancel: Some(cancel.clone()),
            },
            ..Reader::default()
        };

        assert!(cancel_pending_for_transport_replacement(&mut state));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(cancel_pending_for_transport_replacement(&mut state));
        let context = active_reconnect_context(&state);
        assert!(context.contains("future_operation=transport_interrupted_result_unknown"));
        assert!(context.contains("inspect current state"));
        assert!(!context.contains("completed"));
        assert_eq!(
            context
                .matches("future_operation=transport_interrupted_result_unknown")
                .count(),
            1
        );
    }

    #[test]
    fn catalog_fallback_retries_once_after_backoff_then_becomes_terminal() {
        let now = Instant::now();
        let mut recovery = CatalogRecovery::default();

        recovery.catalog_omitted(now);
        assert!(!recovery.retry_due(now));
        assert!(recovery.retry_due(now + CATALOG_RETRY_BACKOFF));

        recovery.begin_retry();
        recovery.catalog_omitted(now + CATALOG_RETRY_BACKOFF);
        assert!(!recovery.retry_due(now + CATALOG_RETRY_BACKOFF * 2));
        assert!(!recovery.retry_available);
        assert!(recovery.terminal_reported);

        recovery.catalog_present();
        assert!(recovery.retry_available);
        assert!(!recovery.terminal_reported);
    }
}
