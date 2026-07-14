//! Identity-bracketed native accessibility state used beside rendered frames.

use super::super::controller::world::SurfaceIdentity;
use super::{UiElement, uia};

pub(super) struct NativePerception {
    pub elements: Vec<UiElement>,
    pub observed: bool,
    pub surface: Option<SurfaceIdentity>,
}

pub(super) fn native_perception(target: Option<&str>) -> NativePerception {
    let before = uia::observe_native_identity(target).ok();
    let elements = uia::enumerate(target);
    let after = before.and_then(|identity| {
        (uia::current_native_identity(target).ok() == Some(identity)).then_some(identity)
    });
    let surface = after.map(|(hwnd, pid, generation)| SurfaceIdentity::Native {
        hwnd,
        pid,
        generation,
    });
    match elements {
        Ok(elements) if surface.is_some() => NativePerception {
            elements,
            observed: true,
            surface,
        },
        _ => NativePerception {
            elements: Vec::new(),
            observed: false,
            surface: None,
        },
    }
}

impl super::Brain {
    pub(super) fn semantic_surface_state(&mut self) -> Option<super::SemanticSurfaceState> {
        if self.controlled_tab_id.is_none() && !super::super::browser::input_active() {
            return None;
        }
        let observed = self.controller.observe();
        if observed.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            return None;
        }
        let elements = observed
            .get("elements")
            .and_then(serde_json::Value::as_str)?;
        let title = observed
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let url = observed
            .get("url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let identity = self.controller.observed_identity()?.clone();
        if let SurfaceIdentity::Browser {
            tab_id,
            document_id,
            ..
        } = &identity
        {
            if self.controlled_tab_id.is_none() {
                if !self.controller.adopt_observed_browser_target(&identity) {
                    return None;
                }
                self.controlled_tab_id = Some(*tab_id);
            }
            if self.controlled_tab_id != Some(*tab_id) {
                return None;
            }
            self.controlled_document_id = Some(document_id.clone());
        }
        super::super::telemetry::human(
            "cc",
            format!(
                "semantic provider=browser_bridge title={:?} url={:?}",
                title.chars().take(70).collect::<String>(),
                url.chars().take(100).collect::<String>()
            ),
        );
        Some(super::SemanticSurfaceState {
            elements: elements.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            identity,
        })
    }
}
