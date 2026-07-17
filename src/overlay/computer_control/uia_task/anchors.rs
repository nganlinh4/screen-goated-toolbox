//! Source-aware click anchors: creation, annotation state, invalidation and
//! fail-closed dispatch. Kept out of the main brain/dispatch files so the
//! lifecycle remains independently testable.

use super::super::controller::world::SurfaceIdentity;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnchorSource {
    Detector,
    VisionMap,
}

impl AnchorSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Detector => "local_ui_detr_1",
            Self::VisionMap => "vision_map",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ClickAnchor {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub note: Option<String>,
    pub verify_description: Option<String>,
    pub source: AnchorSource,
    pub score: Option<f32>,
    pub bounds: Option<[i32; 4]>,
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

    pub(super) fn install_detector_anchors(
        &mut self,
        boxes: Vec<super::super::detector::DetBox>,
        frame_id: u64,
        view: View,
        captured_surface: SurfaceIdentity,
    ) {
        let first_id = self.next_anchor_id;
        if super::frame_identity::validate_current(self.target.as_deref(), &captured_surface)
            .is_err()
        {
            self.clear_anchors("detector_surface_identity_unavailable");
            super::super::telemetry::typed_error(
                "ERR_ANCHOR_SURFACE_IDENTITY",
                "grounding",
                "detector anchors require the captured surface to remain current",
                json!({"frame_id": frame_id, "view": [view.x, view.y, view.w, view.h]}),
            );
            return;
        }
        let surface = captured_surface;
        self.anchors = boxes
            .into_iter()
            .filter_map(labeled_detector_box)
            .enumerate()
            .map(|(index, item)| ClickAnchor {
                id: first_id.saturating_add(index as u32),
                x: item.cx,
                y: item.cy,
                note: item.label.clone(),
                verify_description: None,
                source: AnchorSource::Detector,
                score: Some(item.score),
                bounds: Some([item.left, item.top, item.right, item.bottom]),
                frame_id,
                view,
                surface: surface.clone(),
            })
            .collect();
        self.next_anchor_id = first_id.saturating_add(self.anchors.len() as u32);
        if self.anchors.is_empty() {
            return;
        }
        let anchors: Vec<_> = self
            .anchors
            .iter()
            .map(|anchor| {
                json!({
                    "id": anchor.id,
                    "center": [anchor.x, anchor.y],
                    "bounds": anchor.bounds,
                    "score": anchor.score,
                })
            })
            .collect();
        super::super::telemetry::event(
            "anchor_set_created",
            "grounding",
            super::super::telemetry::Privacy::Safe,
            json!({
                "source": AnchorSource::Detector.as_str(),
                "frame_id": frame_id,
                "view": [view.x, view.y, view.w, view.h],
                "surface": surface,
                "anchors": anchors,
            }),
        );
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
        self.click_mark(args, ctx, cancel, action, step)
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
                    verify_description: Some(description.to_string()),
                    source: AnchorSource::VisionMap,
                    score: None,
                    bounds: None,
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
                "source": AnchorSource::VisionMap.as_str(),
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
        ctx: &str,
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
        let Some(mut anchor) = self.anchors.iter().find(|anchor| anchor.id == id).cloned() else {
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
        if anchor.source == AnchorSource::Detector {
            match refresh_detector_anchor(&anchor) {
                Ok(fresh) => anchor = fresh,
                Err(error) => {
                    self.clear_anchors("click_mark_detector_mismatch");
                    return json!({
                        "ok": false,
                        "error": format!("detector mark is stale: {error}; observe again"),
                    });
                }
            }
        }
        let view_norm = screen_to_view_norm(self.view, anchor.x, anchor.y);
        if let Some(description) = anchor.verify_description.clone()
            && let Err(error) =
                verify_mapped_anchor(self.view, view_norm, &anchor, &description, ctx, cancel)
        {
            self.clear_anchors("mapped_anchor_verification_failed");
            return json!({"ok": false, "error": format!("mapped click verification failed: {error}")});
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
                "anchor_source": anchor.source.as_str(),
                "anchor_frame_id": anchor.frame_id,
                "bounds": anchor.bounds,
                "score": anchor.score,
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
                "anchor_source": anchor.source.as_str(),
                "anchor_frame_id": anchor.frame_id,
            }),
        );
        self.clear_anchors("after_click_mark");
        result
    }
}

fn verify_mapped_anchor(
    view: View,
    view_norm: (f64, f64),
    anchor: &ClickAnchor,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<()> {
    let fresh = session::capture_virtual()?;
    let (fresh_jpeg, _) = session::encode_view(&fresh, view, VISION_SHORT, None, None, None)?;
    verify_located(
        &fresh_jpeg,
        super::super::vision_reader::Located {
            x: view_norm.0,
            y: view_norm.1,
            note: anchor.note.clone(),
        },
        description,
        ctx,
        cancel,
    )?;
    Ok(())
}

pub(super) fn current_surface_identity(target: Option<&str>) -> Option<SurfaceIdentity> {
    super::frame_identity::current_surface(target).ok()
}

fn labeled_detector_box(
    mut item: super::super::detector::DetBox,
) -> Option<super::super::detector::DetBox> {
    let label = item.label.take()?.trim().to_string();
    if label.is_empty() {
        return None;
    }
    item.label = Some(label);
    Some(item)
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

fn refresh_detector_anchor(anchor: &ClickAnchor) -> Result<ClickAnchor> {
    if super::frame_identity::validate_current(None, &anchor.surface).is_err() {
        anyhow::bail!("foreground surface changed before verification");
    }
    let expected = anchor
        .bounds
        .ok_or_else(|| anyhow::anyhow!("detector anchor has no bounds"))?;
    let capture = session::capture_virtual()?;
    if super::frame_identity::validate_current(None, &anchor.surface).is_err() {
        anyhow::bail!("foreground surface changed while capturing verification frame");
    }
    let frame_id = super::super::telemetry::next_frame("detector_anchor_verify");
    let boxes = super::super::detector::detect_capture(&capture, anchor.view, frame_id);
    let (fresh, overlap) = boxes
        .iter()
        .map(|candidate| {
            let bounds = [
                candidate.left,
                candidate.top,
                candidate.right,
                candidate.bottom,
            ];
            (candidate, bounds_iou(expected, bounds))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .ok_or_else(|| anyhow::anyhow!("no clickable regions remain"))?;
    if overlap < 0.35 {
        anyhow::bail!("best fresh overlap was only {overlap:.2}");
    }
    let mark = [(fresh.cx, fresh.cy, anchor.id)];
    let (jpeg, _) =
        session::encode_view(&capture, anchor.view, VISION_SHORT, None, None, Some(&mark))?;
    let labels = super::super::vision_reader::label_clickable_marks(&jpeg, &[anchor.id])?;
    let label = labels
        .into_iter()
        .find_map(|(id, label)| (id == anchor.id).then_some(label))
        .filter(|label| !label.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("fresh candidate is not an enabled actionable control"))?;
    if super::frame_identity::validate_current(None, &anchor.surface).is_err() {
        anyhow::bail!("foreground surface changed during semantic verification");
    }
    let mut refreshed = anchor.clone();
    refreshed.x = fresh.cx;
    refreshed.y = fresh.cy;
    refreshed.bounds = Some([fresh.left, fresh.top, fresh.right, fresh.bottom]);
    refreshed.score = Some(fresh.score);
    refreshed.frame_id = frame_id;
    refreshed.note = Some(label.clone());
    super::super::telemetry::event(
        "detector_anchor_revalidated",
        "grounding",
        super::super::telemetry::Privacy::UserText,
        json!({
            "anchor_id": anchor.id,
            "old_frame_id": anchor.frame_id,
            "fresh_frame_id": frame_id,
            "overlap": overlap,
            "label": label,
        }),
    );
    Ok(refreshed)
}

fn bounds_iou(left: [i32; 4], right: [i32; 4]) -> f32 {
    let intersection_width = (left[2].min(right[2]) - left[0].max(right[0])).max(0) as f32;
    let intersection_height = (left[3].min(right[3]) - left[1].max(right[1])).max(0) as f32;
    let intersection = intersection_width * intersection_height;
    let left_area = ((left[2] - left[0]).max(0) * (left[3] - left[1]).max(0)) as f32;
    let right_area = ((right[2] - right[0]).max(0) * (right[3] - right[1]).max(0)) as f32;
    let union = left_area + right_area - intersection;
    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

pub(super) fn detector_surface_blind(elements: &[UiElement], view: View) -> bool {
    let view_area = i64::from(view.w.max(1)) * i64::from(view.h.max(1));
    let actionable = actionable_elements(elements, view);
    if actionable.is_empty() {
        return true;
    }
    let covered: i64 = actionable
        .iter()
        .map(|element| {
            let width = (element.right.min(view.x + view.w) - element.left.max(view.x)).max(0);
            let height = (element.bottom.min(view.y + view.h) - element.top.max(view.y)).max(0);
            i64::from(width) * i64::from(height)
        })
        .sum();
    actionable.len() <= 12 && covered as f64 / (view_area as f64) < 0.03
}

pub(super) fn accessible_rects(elements: &[UiElement], view: View) -> Vec<[i32; 4]> {
    actionable_elements(elements, view)
        .into_iter()
        .map(|element| [element.left, element.top, element.right, element.bottom])
        .collect()
}

fn actionable_elements(elements: &[UiElement], view: View) -> Vec<&UiElement> {
    elements
        .iter()
        .filter(|element| {
            element.enabled
                && !element.name.trim().is_empty()
                && is_clickable(element.control_type)
                && element.right > view.x
                && element.bottom > view.y
                && element.left < view.x + view.w
                && element.top < view.y + view.h
        })
        .collect()
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
