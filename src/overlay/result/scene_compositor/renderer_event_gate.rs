//! Admit renderer actions only for their desktop-coordinate epoch.
use super::{HOST_HWND, command_delivery, topology_transition};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::HWND;

pub(super) fn accept(message: Option<&serde_json::Value>) -> bool {
    let Some(message) = message else {
        return true;
    };
    if matches!(
        message.get("type").and_then(|value| value.as_str()),
        Some("interaction" | "navigation_request")
    ) && (topology_transition::active() || !topology_transition::accepts_epoch(message))
    {
        return false;
    }
    let Some(action) = message.get("action").and_then(|value| value.as_str()) else {
        return true;
    };
    if action == "scene_batch_complete" {
        if let Some(id) = message
            .get("batch_id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
        {
            command_delivery::complete_batch(id);
        }
        return false;
    }
    if action == "topology_ready" {
        if let Some(epoch) = message.get("epoch").and_then(|value| value.as_str()) {
            topology_transition::ready(HWND(HOST_HWND.load(Ordering::SeqCst) as _), epoch);
        }
        return false;
    }
    topology_transition::accepts_epoch(message)
        && (!topology_transition::active()
            || matches!(
                action,
                "update_clickable_regions"
                    | "processing_control_regions"
                    | "update_refine_draft"
                    | "release_refine_focus"
            ))
}
