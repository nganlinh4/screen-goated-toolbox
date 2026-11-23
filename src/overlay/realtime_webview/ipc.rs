//! Renderer IPC. Geometry stays local for pointer-rate smoothness; semantic
//! actions are forwarded to the authoritative desktop parent.

use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

use super::layout::{self, CardRole};
use super::protocol::ChildEvent;

#[derive(Deserialize)]
struct IpcEnvelope {
    role: String,
    body: String,
    #[serde(default = "unit_scale")]
    scale: f64,
    #[serde(default)]
    session_id: String,
}

static CURRENT_SESSION: AtomicU64 = AtomicU64::new(0);

pub(super) fn is_current_session(session_id: u64) -> bool {
    session_id != 0 && current_session() == session_id
}

pub(super) fn current_session() -> u64 {
    CURRENT_SESSION.load(Ordering::SeqCst)
}

pub(super) fn install_session(session_id: u64) -> Option<bool> {
    loop {
        let current = CURRENT_SESSION.load(Ordering::SeqCst);
        if !session_transition(current, session_id)? {
            return Some(false);
        }
        if CURRENT_SESSION
            .compare_exchange(current, session_id, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Some(true);
        }
    }
}

fn session_transition(current: u64, incoming: u64) -> Option<bool> {
    (incoming >= current).then_some(incoming != current)
}

fn admitted_session(raw: &str, current: u64) -> Option<u64> {
    let session = raw.parse::<u64>().ok()?;
    (session != 0 && session == current).then_some(session)
}

fn unit_scale() -> f64 {
    1.0
}

pub(super) fn handle(hwnd: HWND, raw: &str) {
    let Ok(message) = serde_json::from_str::<IpcEnvelope>(raw) else {
        super::child::emit_event(ChildEvent::RendererError {
            source: "ipc".to_string(),
            error: format!("malformed message bytes={}", raw.len()),
        });
        return;
    };
    if message.role == "compositor" && message.body == "ready" {
        super::child::renderer_ready();
        return;
    }
    if message.role == "compositor" {
        if let Some(request) = message
            .body
            .strip_prefix("layout_painted:")
            .and_then(|value| value.parse().ok())
        {
            super::webview::reveal_after_paint(hwnd, request);
        }
        return;
    }
    let Some(role) = CardRole::parse(&message.role) else {
        return;
    };
    let Some(session_id) =
        admitted_session(&message.session_id, CURRENT_SESSION.load(Ordering::SeqCst))
    else {
        return;
    };
    handle_card_message(hwnd, role, &message.body, message.scale, session_id);
}

fn handle_card_message(hwnd: HWND, role: CardRole, body: &str, scale: f64, session_id: u64) {
    if body == "textInputStart" {
        super::text_input_focus::begin(hwnd, role, session_id);
    } else if body == "textInputEnd" {
        super::text_input_focus::end(hwnd);
    } else if body == "interactionStart" {
        layout::set_interaction_active(hwnd, true);
    } else if body == "interactionEnd" {
        layout::set_interaction_active(hwnd, false);
        emit_layout(session_id);
    } else if let Some((dx, dy)) = parse_delta(body, "cardDragMove:", scale) {
        layout::move_card(role, dx, dy);
        super::webview::sync_compositor_layout(hwnd);
    } else if let Some((dx, dy)) = parse_delta(body, "groupDragMove:", scale) {
        layout::move_group(dx, dy);
        super::webview::sync_compositor_layout(hwnd);
    } else if let Some((dx, dy)) = parse_delta(body, "resize:", scale) {
        layout::resize_card(role, dx, dy);
        super::webview::sync_compositor_layout(hwnd);
    } else if let Some(visible) = parse_toggle(body, "toggleMic:") {
        layout::set_visible(CardRole::Transcription, visible);
        sync_layout(hwnd, session_id);
        emit_input(role, body, scale, session_id);
    } else if let Some(visible) = parse_toggle(body, "toggleTrans:") {
        layout::set_visible(CardRole::Translation, visible);
        sync_layout(hwnd, session_id);
        emit_input(role, body, scale, session_id);
    } else if let Some(text) = body.strip_prefix("copyText:") {
        crate::overlay::utils::copy_to_clipboard(text, hwnd);
    } else if body == "close" {
        super::text_input_focus::end(hwnd);
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        super::child::emit_event(ChildEvent::Close { session_id });
    } else {
        emit_input(role, body, scale, session_id);
    }
}

fn sync_layout(hwnd: HWND, session_id: u64) {
    super::webview::sync_compositor_layout(hwnd);
    emit_layout(session_id);
}

fn emit_layout(session_id: u64) {
    super::child::emit_event(ChildEvent::LayoutChanged {
        session_id,
        layout: layout::snapshot(),
    });
}

fn emit_input(role: CardRole, body: &str, scale: f64, session_id: u64) {
    super::child::emit_event(ChildEvent::Input {
        session_id,
        role,
        body: body.to_string(),
        scale,
    });
}

fn parse_delta(body: &str, prefix: &str, scale: f64) -> Option<(i32, i32)> {
    let (x, y) = body.strip_prefix(prefix)?.split_once(',')?;
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    Some((
        (x.parse::<f64>().ok()? * scale).round() as i32,
        (y.parse::<f64>().ok()? * scale).round() as i32,
    ))
}

fn parse_toggle(body: &str, prefix: &str) -> Option<bool> {
    match body.strip_prefix(prefix)? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{admitted_session, parse_delta, parse_toggle, session_transition};

    #[test]
    fn stale_and_missing_session_messages_are_rejected() {
        assert_eq!(admitted_session("42", 42), Some(42));
        assert_eq!(admitted_session("41", 42), None);
        assert_eq!(admitted_session("", 42), None);
        assert_eq!(admitted_session("0", 0), None);
        assert_eq!(session_transition(42, 41), None);
        assert_eq!(session_transition(42, 42), Some(false));
        assert_eq!(session_transition(42, 43), Some(true));
    }

    #[test]
    fn compositor_deltas_are_converted_to_physical_pixels() {
        assert_eq!(parse_delta("resize:4,-2", "resize:", 1.5), Some((6, -3)));
        assert_eq!(parse_toggle("toggleMic:1", "toggleMic:"), Some(true));
    }
}
