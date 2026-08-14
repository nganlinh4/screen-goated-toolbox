//! Renderer IPC. Geometry stays local for pointer-rate smoothness; semantic
//! actions are forwarded to the authoritative desktop parent.

use serde::Deserialize;
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
    let Some(role) = CardRole::parse(&message.role) else {
        return;
    };
    handle_card_message(hwnd, role, &message.body, message.scale);
}

fn handle_card_message(hwnd: HWND, role: CardRole, body: &str, scale: f64) {
    if body == "interactionStart" {
        layout::set_interaction_active(hwnd, true);
    } else if body == "interactionEnd" {
        layout::set_interaction_active(hwnd, false);
        emit_layout();
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
        sync_layout(hwnd);
        emit_input(role, body, scale);
    } else if let Some(visible) = parse_toggle(body, "toggleTrans:") {
        layout::set_visible(CardRole::Translation, visible);
        sync_layout(hwnd);
        emit_input(role, body, scale);
    } else if let Some(text) = body.strip_prefix("copyText:") {
        crate::overlay::utils::copy_to_clipboard(text, hwnd);
    } else if body == "close" {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        super::child::emit_event(ChildEvent::Close);
    } else {
        emit_input(role, body, scale);
    }
}

fn sync_layout(hwnd: HWND) {
    super::webview::sync_compositor_layout(hwnd);
    emit_layout();
}

fn emit_layout() {
    super::child::emit_event(ChildEvent::LayoutChanged {
        layout: layout::snapshot(),
    });
}

fn emit_input(role: CardRole, body: &str, scale: f64) {
    super::child::emit_event(ChildEvent::Input {
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
    use super::{parse_delta, parse_toggle};

    #[test]
    fn compositor_deltas_are_converted_to_physical_pixels() {
        assert_eq!(parse_delta("resize:4,-2", "resize:", 1.5), Some((6, -3)));
        assert_eq!(parse_toggle("toggleMic:1", "toggleMic:"), Some(true));
    }
}
