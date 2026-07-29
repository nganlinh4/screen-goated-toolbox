//! Frame-bound visual marks: creation, annotation state, invalidation, local
//! pixel revalidation, and fail-closed dispatch.

use super::super::controller::world::SurfaceIdentity;
use super::*;

#[derive(Clone, Debug)]
pub(super) struct ClickAnchor {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub note: Option<String>,
    pub signature: Vec<u8>,
    pub frame_id: u64,
    pub view: View,
    pub surface: SurfaceIdentity,
}

impl Brain {
    pub(super) fn anchor_marks(&self) -> Vec<(i32, i32, u32)> {
        self.anchors
            .iter()
            .map(|anchor| (anchor.x, anchor.y, anchor.id))
            .collect()
    }

    pub(super) fn clear_anchors(&mut self, reason: &str) {
        if self.anchors.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.anchors.iter().map(|anchor| anchor.id).collect();
        super::super::telemetry::event(
            "anchor_set_invalidated",
            "grounding",
            super::super::telemetry::Privacy::Safe,
            json!({"reason": reason, "anchor_ids": ids}),
        );
        self.anchors.clear();
    }

    pub(super) fn marks_state(&self) -> Option<String> {
        if self.anchors.is_empty() {
            return None;
        }
        let mut state = String::from(
            "CLICKABLE MARKS (the same numbers are drawn on the frame; use click_mark):\n",
        );
        for anchor in &self.anchors {
            let what = anchor.note.as_deref().unwrap_or("clickable");
            state.push_str(&format!("[{}] {what}\n", anchor.id));
        }
        eprintln!("[cc] {} clickable marks", self.anchors.len());
        Some(state)
    }

    pub(super) fn dispatch_anchor_action(
        &mut self,
        name: &str,
        args: &Value,
        ctx: &str,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
        step: usize,
    ) -> Value {
        if name == "map_targets" {
            return self.map_targets(args, ctx, cancel, step);
        }
        self.click_mark(args, cancel, action, step)
    }

    fn map_targets(&mut self, args: &Value, ctx: &str, cancel: &AtomicBool, step: usize) -> Value {
        let description = args
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        self.clear_anchors("before_map_targets");
        let Some(surface) = current_surface_identity(self.target.as_deref()) else {
            return json!({"ok": false, "error": "cannot bind click anchors to the current surface"});
        };
        let points = match map_in_view(self.view, description, ctx, cancel) {
            Ok(points) => points,
            Err(error) => {
                return json!({"ok": false, "error": format!("could not map '{description}': {error}")});
            }
        };
        if super::frame_identity::validate_current(self.target.as_deref(), &surface).is_err() {
            return json!({
                "ok": false,
                "error": "the surface changed while targets were being mapped; observe again",
            });
        }
        let first_id = self.next_anchor_id;
        self.anchors = points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let (x, y) = self.view.to_screen_px(point.x, point.y);
                ClickAnchor {
                    id: first_id.saturating_add(index as u32),
                    x,
                    y,
                    note: point.note.clone(),
                    signature: point.signature.clone(),
                    frame_id: 0,
                    view: self.view,
                    surface: surface.clone(),
                }
            })
            .collect();
        self.next_anchor_id = first_id.saturating_add(self.anchors.len() as u32);
        let list: Vec<Value> = self
            .anchors
            .iter()
            .map(|anchor| json!({"mark": anchor.id, "what": anchor.note}))
            .collect();
        super::super::telemetry::event(
            "anchor_set_created",
            "grounding",
            super::super::telemetry::Privacy::Safe,
            json!({
                "source": "current_frame_vision",
                "view": [self.view.x, self.view.y, self.view.w, self.view.h],
                "anchor_ids": self.anchors.iter().map(|anchor| anchor.id).collect::<Vec<_>>(),
            }),
        );
        eprintln!(
            "[cc] step {step:02} MAP_TARGETS '{description}' -> {} anchors",
            self.anchors.len()
        );
        json!({
            "ok": true,
            "anchor_count": self.anchors.len(),
            "anchors": list,
            "note": "Use click_mark on a current numbered mark. Mutating actions invalidate the set.",
        })
    }

    fn click_mark(
        &mut self,
        args: &Value,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
        step: usize,
    ) -> Value {
        let id = args.get("mark").and_then(Value::as_u64).unwrap_or(0) as u32;
        let button = if args.get("button").and_then(Value::as_str) == Some("right") {
            "right"
        } else {
            "left"
        };
        let Some(anchor) = self.anchors.iter().find(|anchor| anchor.id == id).cloned() else {
            return json!({
                "ok": false,
                "error": format!("no current anchor #{id} (have {}); observe/map again", self.anchors.len()),
            });
        };
        let current_view = if self.zoomed {
            self.view
        } else {
            window_view(self.target.as_deref(), self.whole_screen)
        };
        let current_view = clamp_to_virtual_desktop(current_view);
        if !same_view(anchor.view, current_view) {
            self.clear_anchors("click_mark_view_changed");
            super::super::telemetry::event(
                "anchor_click_rejected",
                "grounding",
                super::super::telemetry::Privacy::Safe,
                json!({
                    "anchor_id": id,
                    "reason": "view_changed",
                    "expected_view": [anchor.view.x, anchor.view.y, anchor.view.w, anchor.view.h],
                    "current_view": [current_view.x, current_view.y, current_view.w, current_view.h],
                }),
            );
            return json!({
                "ok": false,
                "error": "click mark is stale because the target view moved or resized; observe/map again",
            });
        }
        let current_surface = current_surface_identity(self.target.as_deref());
        if current_surface.as_ref() != Some(&anchor.surface) {
            self.clear_anchors("click_mark_surface_changed");
            return json!({
                "ok": false,
                "error": "click mark is stale because the foreground surface changed; observe/map again",
            });
        }
        if anchor.frame_id == 0 {
            self.clear_anchors("click_mark_unpublished");
            return json!({
                "ok": false,
                "error": "click mark has not been published on a current frame; observe/map again",
            });
        }
        let view_norm = screen_to_view_norm(self.view, anchor.x, anchor.y);
        let fresh = match session::capture_virtual() {
            Ok(capture) => capture,
            Err(error) => {
                self.clear_anchors("click_mark_capture_failed");
                return json!({"ok": false, "error": format!("could not revalidate click mark: {error}")});
            }
        };
        let current_signature = session::target_region_fingerprint(
            &fresh,
            anchor.x,
            anchor.y,
            GROUNDING_SIGNATURE_HALF,
        );
        if !session::target_fingerprint_matches(&anchor.signature, &current_signature) {
            self.clear_anchors("click_mark_pixels_changed");
            return json!({
                "ok": false,
                "error": "click mark is stale because its target pixels changed; map again",
            });
        }
        let latest_view = if self.zoomed {
            self.view
        } else {
            clamp_to_virtual_desktop(window_view(self.target.as_deref(), self.whole_screen))
        };
        if !same_view(anchor.view, latest_view)
            || current_surface_identity(self.target.as_deref()).as_ref() != Some(&anchor.surface)
        {
            self.clear_anchors("click_mark_context_changed_during_verification");
            return json!({
                "ok": false,
                "error": "click mark became stale while it was being verified; observe again",
            });
        }
        self.last_click = Some((anchor.x, anchor.y));
        self.click_before = session::capture_region_fp(anchor.x, anchor.y, VC_HALF);
        append_click(
            &self.dir,
            action,
            json!({
                "step": step,
                "kind": "click_mark",
                "mark": id,
                "button": button,
                "view_norm": [view_norm.0, view_norm.1],
                "screen_px": [anchor.x, anchor.y],
                "saw": anchor.note,
                "anchor_source": "current_frame_vision",
                "anchor_frame_id": anchor.frame_id,
                "view_rect": [self.view.x, self.view.y, self.view.w, self.view.h],
            }),
        );
        eprintln!(
            "[cc] step {step:02} CLICK_MARK {id} -> screen({},{})",
            anchor.x, anchor.y
        );
        let source = FrameSource {
            frame_id: anchor.frame_id,
            surface: anchor.surface.clone(),
        };
        let input = click_screen(
            anchor.x,
            anchor.y,
            button,
            InputContext {
                dry: self.dry,
                profile: &self.profile,
                cancel,
                target: self.target.as_deref(),
                source: Some(&source),
            },
        );
        let result = pointer_result(
            input,
            self.view,
            view_norm,
            (anchor.x, anchor.y),
            json!({
                "kind": "click_mark",
                "clicked_mark": id,
                "what": anchor.note,
                "anchor_source": "current_frame_vision",
                "anchor_frame_id": anchor.frame_id,
            }),
        );
        self.clear_anchors("after_click_mark");
        result
    }
}

pub(super) fn current_surface_identity(target: Option<&str>) -> Option<SurfaceIdentity> {
    super::frame_identity::current_surface(target).ok()
}

pub(super) fn has_accessible_action(elements: &[UiElement], view: View) -> bool {
    elements.iter().any(|element| {
        element.enabled
            && !element.name.trim().is_empty()
            && is_clickable(element.control_type)
            && element.right > view.x
            && element.bottom > view.y
            && element.left < view.x + view.w
            && element.top < view.y + view.h
    })
}

pub(super) fn clamp_to_virtual_desktop(view: View) -> View {
    let (desktop_x, desktop_y, desktop_w, desktop_h) = uia::virtual_desktop();
    let left = view.x.max(desktop_x);
    let top = view.y.max(desktop_y);
    let right = (view.x + view.w).min(desktop_x + desktop_w);
    let bottom = (view.y + view.h).min(desktop_y + desktop_h);
    View {
        x: left,
        y: top,
        w: (right - left).max(1),
        h: (bottom - top).max(1),
    }
}

pub(super) fn action_invalidates_anchors(name: &str) -> bool {
    !matches!(
        name,
        "observe"
            | "look"
            | "list_windows"
            | "read_clipboard"
            | "search_memory"
            | "open_memory"
            | "list_files"
            | "read_text_file"
            | "system_query"
            | "artifact_info"
            | "extract_artifact"
            | "browser_status"
            | "browser_read_page"
            | "browser_extract_page"
            | "browser_tabs"
            | "browser_network"
            | "browser_console"
            | "list_app_integrations"
            | "app_integration_status"
            | "read_app_integration_docs"
            | "map_targets"
            | "click_mark"
    )
}

pub(super) fn same_view(left: View, right: View) -> bool {
    left.x == right.x && left.y == right.y && left.w == right.w && left.h == right.h
}

#[cfg(test)]
mod tests;

mod lifecycle;
