//! Transactional trusted pointer input for an exact browser tab.
//!
//! A press is the effect boundary. Before it, cancellation is a proven no-op.
//! After it, every exit performs a bounded best-effort release that deliberately
//! ignores cancellation, then reports the effect as uncertain when completion is
//! not proven.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::{Value, json};

use super::super::controller::world::BrowserWindowIdentity;

const GRAB_HOLD: Duration = Duration::from_millis(110);
const DROP_HOLD: Duration = Duration::from_millis(110);
const GLIDE_STEP_HOLD: Duration = Duration::from_millis(14);
const GLIDE_STEPS: i32 = 28;
const CANCEL_POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq)]
enum PointerCommand {
    Move { x: f64, y: f64, held: bool },
    Press { x: f64, y: f64, right: bool },
    Release { x: f64, y: f64, right: bool },
}

impl PointerCommand {
    fn params(self) -> Value {
        match self {
            Self::Move { x, y, held: false } => {
                json!({"type":"mouseMoved","x":x,"y":y})
            }
            Self::Move { x, y, held: true } => {
                json!({"type":"mouseMoved","x":x,"y":y,"button":"left","buttons":1})
            }
            Self::Press { x, y, right } => {
                let (button, buttons) = button_fields(right);
                json!({"type":"mousePressed","x":x,"y":y,"button":button,"buttons":buttons,"clickCount":1})
            }
            Self::Release { x, y, right } => {
                let (button, _) = button_fields(right);
                json!({"type":"mouseReleased","x":x,"y":y,"button":button,"buttons":0,"clickCount":1})
            }
        }
    }
}

fn button_fields(right: bool) -> (&'static str, i32) {
    if right { ("right", 2) } else { ("left", 1) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DispatchMode {
    Normal,
    Cleanup,
}

#[derive(Debug)]
struct PointerInputError {
    stage: &'static str,
    detail: String,
    cancelled: bool,
    effect_may_have_occurred: bool,
    release_attempted: bool,
    release_succeeded: Option<bool>,
}

impl PointerInputError {
    fn before_effect(stage: &'static str, detail: impl fmt::Display, cancelled: bool) -> Self {
        Self {
            stage,
            detail: detail.to_string(),
            cancelled,
            effect_may_have_occurred: false,
            release_attempted: false,
            release_succeeded: None,
        }
    }

    fn after_effect(
        stage: &'static str,
        detail: impl fmt::Display,
        cancelled: bool,
        release: &Result<()>,
    ) -> Self {
        let release_detail = release
            .as_ref()
            .err()
            .map(|error| format!("; cleanup release failed: {error}"))
            .unwrap_or_default();
        Self {
            stage,
            detail: format!("{detail}{release_detail}"),
            cancelled,
            effect_may_have_occurred: true,
            release_attempted: true,
            release_succeeded: Some(release.is_ok()),
        }
    }

    fn release_failed(error: anyhow::Error, cancelled: bool) -> Self {
        let detail = error.to_string();
        let failed: Result<()> = Err(anyhow::anyhow!(detail.clone()));
        Self::after_effect("release", detail, cancelled, &failed)
    }
}

impl fmt::Display for PointerInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "browser pointer {}: {}", self.stage, self.detail)
    }
}

impl std::error::Error for PointerInputError {}

pub(in crate::overlay::computer_control) fn cancelled_before_pointer_effect(
    stage: &'static str,
) -> Value {
    error_value(&PointerInputError::before_effect(
        stage,
        "cancelled by user",
        true,
    ))
}

pub(in crate::overlay::computer_control) fn pointer_error_response(error: anyhow::Error) -> Value {
    match error.downcast_ref::<PointerInputError>() {
        Some(error) => error_value(error),
        None => super::errors::response(error),
    }
}

fn error_value(error: &PointerInputError) -> Value {
    json!({
        "ok": false,
        "code": if error.cancelled {
            "ERR_BROWSER_POINTER_CANCELLED"
        } else if error.effect_may_have_occurred {
            "ERR_BROWSER_POINTER_EFFECT_UNCERTAIN"
        } else {
            "ERR_BROWSER_POINTER_FAILED"
        },
        "status": if error.cancelled { "aborted_by_user" } else { "failed" },
        "cancelled": error.cancelled,
        "stage": error.stage,
        "effect_may_have_occurred": error.effect_may_have_occurred,
        "release_attempted": error.release_attempted,
        "release_succeeded": error.release_succeeded,
        "error": error.to_string(),
    })
}

pub(in crate::overlay::computer_control) fn click_on_document(
    x: f64,
    y: f64,
    right: bool,
    tab_id: i64,
    document_id: &str,
    window: &BrowserWindowIdentity,
    cancel: &AtomicBool,
) -> Result<()> {
    run_click_with(
        x,
        y,
        right,
        cancel,
        |command, mode| dispatch(tab_id, command, mode),
        || super::validate_active_document_identity(tab_id, document_id, window),
    )
}

fn run_click_with(
    x: f64,
    y: f64,
    right: bool,
    cancel: &AtomicBool,
    mut dispatch: impl FnMut(PointerCommand, DispatchMode) -> Result<()>,
    mut validate: impl FnMut() -> Result<()>,
) -> Result<()> {
    dispatch(
        PointerCommand::Move { x, y, held: false },
        DispatchMode::Normal,
    )
    .map_err(|error| PointerInputError::before_effect("move", error, cancelled(cancel)))?;
    validate().map_err(|error| {
        PointerInputError::before_effect("document_validation", error, cancelled(cancel))
    })?;
    ensure_not_cancelled(cancel, "before_press")?;

    let release = PointerCommand::Release { x, y, right };
    if let Err(error) = dispatch(PointerCommand::Press { x, y, right }, DispatchMode::Normal) {
        if super::bridge_wait::cancellation_effect(&error) == Some(false) {
            return Err(PointerInputError::before_effect("press", error, true).into());
        }
        let cleanup = dispatch(release, DispatchMode::Cleanup);
        let dispatch_cancelled = super::bridge_wait::cancellation_effect(&error).is_some();
        return Err(PointerInputError::after_effect(
            "press",
            error,
            cancelled(cancel) || dispatch_cancelled,
            &cleanup,
        )
        .into());
    }

    let was_cancelled = cancelled(cancel);
    let cleanup = dispatch(release, DispatchMode::Cleanup);
    if was_cancelled || cancelled(cancel) {
        return Err(PointerInputError::after_effect(
            "release",
            "cancelled after press",
            true,
            &cleanup,
        )
        .into());
    }
    cleanup.map_err(|error| PointerInputError::release_failed(error, false))?;
    Ok(())
}

pub(in crate::overlay::computer_control) fn drag_on_document(
    from: (f64, f64),
    to: (f64, f64),
    tab_id: i64,
    document_id: &str,
    window: &BrowserWindowIdentity,
    cancel: &AtomicBool,
) -> Result<()> {
    run_drag_with(
        from,
        to,
        cancel,
        |command, mode| dispatch(tab_id, command, mode),
        || super::validate_active_document_identity(tab_id, document_id, window),
        |duration| pause_cancelled(cancel, duration),
    )
}

fn run_drag_with(
    from: (f64, f64),
    to: (f64, f64),
    cancel: &AtomicBool,
    mut dispatch: impl FnMut(PointerCommand, DispatchMode) -> Result<()>,
    mut validate: impl FnMut() -> Result<()>,
    mut pause: impl FnMut(Duration) -> bool,
) -> Result<()> {
    dispatch(
        PointerCommand::Move {
            x: from.0,
            y: from.1,
            held: false,
        },
        DispatchMode::Normal,
    )
    .map_err(|error| PointerInputError::before_effect("move", error, cancelled(cancel)))?;
    validate().map_err(|error| {
        PointerInputError::before_effect("document_validation", error, cancelled(cancel))
    })?;
    ensure_not_cancelled(cancel, "before_press")?;

    let press = PointerCommand::Press {
        x: from.0,
        y: from.1,
        right: false,
    };
    if let Err(error) = dispatch(press, DispatchMode::Normal) {
        if super::bridge_wait::cancellation_effect(&error) == Some(false) {
            return Err(PointerInputError::before_effect("press", error, true).into());
        }
        let cleanup = dispatch(
            PointerCommand::Release {
                x: from.0,
                y: from.1,
                right: false,
            },
            DispatchMode::Cleanup,
        );
        let dispatch_cancelled = super::bridge_wait::cancellation_effect(&error).is_some();
        return Err(PointerInputError::after_effect(
            "press",
            error,
            cancelled(cancel) || dispatch_cancelled,
            &cleanup,
        )
        .into());
    }

    let mut last = from;
    let body = drag_body(
        from,
        to,
        cancel,
        &mut dispatch,
        &mut validate,
        &mut pause,
        &mut last,
    );
    let cleanup = dispatch(
        PointerCommand::Release {
            x: last.0,
            y: last.1,
            right: false,
        },
        DispatchMode::Cleanup,
    );
    match body {
        Err((stage, error, body_cancelled)) => Err(PointerInputError::after_effect(
            stage,
            error,
            body_cancelled || cancelled(cancel),
            &cleanup,
        )
        .into()),
        Ok(()) if cancelled(cancel) => {
            Err(
                PointerInputError::after_effect("release", "cancelled after press", true, &cleanup)
                    .into(),
            )
        }
        Ok(()) => cleanup.map_err(|error| PointerInputError::release_failed(error, false).into()),
    }
}

type DragBodyError = (&'static str, anyhow::Error, bool);

fn drag_body(
    from: (f64, f64),
    to: (f64, f64),
    cancel: &AtomicBool,
    dispatch: &mut impl FnMut(PointerCommand, DispatchMode) -> Result<()>,
    validate: &mut impl FnMut() -> Result<()>,
    pause: &mut impl FnMut(Duration) -> bool,
    last: &mut (f64, f64),
) -> std::result::Result<(), DragBodyError> {
    if pause(GRAB_HOLD) || cancelled(cancel) {
        return Err(("grab_hold", anyhow::anyhow!("cancelled by user"), true));
    }
    for i in 1..=GLIDE_STEPS {
        if cancelled(cancel) {
            return Err(("glide", anyhow::anyhow!("cancelled by user"), true));
        }
        let t = f64::from(i) / f64::from(GLIDE_STEPS);
        let next = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        dispatch(
            PointerCommand::Move {
                x: next.0,
                y: next.1,
                held: true,
            },
            DispatchMode::Normal,
        )
        .map_err(|error| {
            let was_cancelled =
                cancelled(cancel) || super::bridge_wait::cancellation_effect(&error).is_some();
            ("glide", error, was_cancelled)
        })?;
        *last = next;
        if pause(GLIDE_STEP_HOLD) || cancelled(cancel) {
            return Err(("glide_hold", anyhow::anyhow!("cancelled by user"), true));
        }
    }
    if pause(DROP_HOLD) || cancelled(cancel) {
        return Err(("drop_hold", anyhow::anyhow!("cancelled by user"), true));
    }
    validate().map_err(|error| {
        let was_cancelled =
            cancelled(cancel) || super::bridge_wait::cancellation_effect(&error).is_some();
        ("document_validation", error, was_cancelled)
    })?;
    Ok(())
}

fn ensure_not_cancelled(cancel: &AtomicBool, stage: &'static str) -> Result<()> {
    if cancelled(cancel) {
        Err(PointerInputError::before_effect(stage, "cancelled by user", true).into())
    } else {
        Ok(())
    }
}

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::SeqCst)
}

fn pause_cancelled(cancel: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if cancelled(cancel) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        std::thread::sleep(CANCEL_POLL.min(deadline.saturating_duration_since(now)));
    }
}

fn dispatch(tab_id: i64, command: PointerCommand, mode: DispatchMode) -> Result<()> {
    let params = command.params();
    match mode {
        DispatchMode::Normal => {
            super::bridge::cdp_on_tab("Input.dispatchMouseEvent", params, tab_id)?;
        }
        DispatchMode::Cleanup => cleanup_dispatch(tab_id, params)?,
    }
    Ok(())
}

fn cleanup_dispatch(tab_id: i64, params: Value) -> Result<()> {
    super::capabilities::require(super::capabilities::CDP)?;
    super::capabilities::require(super::capabilities::CDP_EXPLICIT_TAB)?;
    let response = super::bridge::request_cleanup(json!({
        "id": super::bridge::next_request_id(),
        "type": "cdp",
        "method": "Input.dispatchMouseEvent",
        "params": params,
        "tabId": tab_id,
        "requireActive": false,
    }))?;
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(super::bridge::response_error(
            &response,
            "pointer cleanup dispatch failed",
        ))
    }
}

#[cfg(test)]
mod tests;
