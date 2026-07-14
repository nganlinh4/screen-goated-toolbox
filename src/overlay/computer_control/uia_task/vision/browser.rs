//! Vision targeting whose screenshot, verification, and input share one browser-tab route.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde_json::{Value, json};

use super::{run_cancellable, verify_located};
use crate::overlay::computer_control::controller::world::BrowserWindowIdentity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::overlay::computer_control::uia_task) enum BrowserVisionTarget {
    Exact {
        tab_id: i64,
        document_id: String,
        window: BrowserWindowIdentity,
    },
}

impl BrowserVisionTarget {
    fn shot(&self) -> Result<(Vec<u8>, f64, f64)> {
        let Self::Exact {
            tab_id,
            document_id,
            window,
        } = self;
        super::super::super::browser::validate_active_document_identity(
            *tab_id,
            document_id,
            window,
        )?;
        let shot = super::super::super::browser::shot_on_tab(*tab_id)?;
        super::super::super::browser::validate_active_document_identity(
            *tab_id,
            document_id,
            window,
        )?;
        Ok(shot)
    }

    fn click(&self, x: f64, y: f64, right: bool, cancel: &AtomicBool) -> Result<()> {
        let Self::Exact {
            tab_id,
            document_id,
            window,
        } = self;
        super::super::super::browser::click_on_document(
            x,
            y,
            right,
            *tab_id,
            document_id,
            window,
            cancel,
        )
    }

    fn drag(&self, fx: f64, fy: f64, tx: f64, ty: f64, cancel: &AtomicBool) -> Result<()> {
        let Self::Exact {
            tab_id,
            document_id,
            window,
        } = self;
        super::super::super::browser::drag_on_document(
            (fx, fy),
            (tx, ty),
            *tab_id,
            document_id,
            window,
            cancel,
        )
    }

    fn tag(&self, mut value: Value) -> Value {
        let Self::Exact { tab_id, .. } = self;
        if let Some(object) = value.as_object_mut() {
            object.insert("target_tab_id".to_string(), json!(tab_id));
        }
        value
    }
}

pub(in crate::overlay::computer_control::uia_task) fn browser_vision_target(
    controlled_tab_id: Option<i64>,
    source: Option<&super::super::FrameSource>,
) -> Result<Option<BrowserVisionTarget>> {
    let Some(source) = source else {
        anyhow::bail!("model-visible source frame identity is unavailable");
    };
    match &source.surface {
        super::super::super::controller::world::SurfaceIdentity::Browser {
            tab_id,
            document_id,
            window,
        } => {
            if controlled_tab_id.is_some_and(|controlled| controlled != *tab_id) {
                anyhow::bail!("browser source frame does not match the turn's controlled tab");
            }
            Ok(Some(BrowserVisionTarget::Exact {
                tab_id: *tab_id,
                document_id: document_id.clone(),
                window: *window,
            }))
        }
        super::super::super::controller::world::SurfaceIdentity::Native { .. } => Ok(None),
    }
}

fn locate_css(
    target: &BrowserVisionTarget,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<(f64, f64, Option<String>)> {
    let (jpeg, width, height) = target.shot()?;
    let (description_owned, ctx_owned) = (description.to_string(), ctx.to_string());
    let located = run_cancellable(cancel, move || {
        super::super::super::vision_reader::locate_point(&jpeg, &description_owned, &ctx_owned)
    })?;
    let (fresh_jpeg, fresh_width, fresh_height) = target.shot()?;
    if (fresh_width - width).abs() > f64::EPSILON || (fresh_height - height).abs() > f64::EPSILON {
        anyhow::bail!("browser viewport changed while locating the target");
    }
    let located = verify_located(&fresh_jpeg, located, description, ctx, cancel)?;
    Ok((
        located.x / 1000.0 * width,
        located.y / 1000.0 * height,
        located.note,
    ))
}

pub(in crate::overlay::computer_control::uia_task) fn browser_click(
    target: BrowserVisionTarget,
    description: &str,
    right: bool,
    ctx: &str,
    cancel: &AtomicBool,
) -> Value {
    let result = match locate_css(&target, description, ctx, cancel) {
        Ok((x, y, note)) => {
            eprintln!(
                "[cc] CLICK_TARGET(browser) '{description}' -> css({x:.0},{y:.0}) saw={note:?}"
            );
            if cancel.load(Ordering::SeqCst) {
                return target.tag(
                    super::super::super::browser::cancelled_before_pointer_effect(
                        "vision_to_pointer_handoff",
                    ),
                );
            }
            match target.click(x, y, right, cancel) {
                Ok(()) => json!({
                    "ok": true, "via": "browser", "css_px": [x.round(), y.round()],
                    "saw_at_target": note,
                }),
                Err(error) => super::super::super::browser::pointer_error_response(error),
            }
        }
        Err(_) if cancel.load(Ordering::SeqCst) => {
            super::super::super::browser::cancelled_before_pointer_effect("vision_targeting")
        }
        Err(error) => json!({
            "ok": false,
            "code": "ERR_BROWSER_POINTER_TARGETING_FAILED",
            "stage": "vision_targeting",
            "effect_may_have_occurred": false,
            "error": format!("could not locate '{description}': {error}"),
        }),
    };
    target.tag(result)
}

pub(in crate::overlay::computer_control::uia_task) fn browser_drag(
    target: BrowserVisionTarget,
    from: &str,
    to: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Value {
    let from_point = match locate_css(&target, from, ctx, cancel) {
        Ok(value) => value,
        Err(_) if cancel.load(Ordering::SeqCst) => {
            return target.tag(
                super::super::super::browser::cancelled_before_pointer_effect("vision_targeting"),
            );
        }
        Err(error) => {
            return target.tag(json!({
                "ok": false,
                "code": "ERR_BROWSER_POINTER_TARGETING_FAILED",
                "stage": "vision_targeting",
                "effect_may_have_occurred": false,
                "error": format!("could not locate from '{from}': {error}"),
            }));
        }
    };
    let to_point = match locate_css(&target, to, ctx, cancel) {
        Ok(value) => value,
        Err(_) if cancel.load(Ordering::SeqCst) => {
            return target.tag(
                super::super::super::browser::cancelled_before_pointer_effect("vision_targeting"),
            );
        }
        Err(error) => {
            return target.tag(json!({
                "ok": false,
                "code": "ERR_BROWSER_POINTER_TARGETING_FAILED",
                "stage": "vision_targeting",
                "effect_may_have_occurred": false,
                "error": format!("could not locate to '{to}': {error}"),
            }));
        }
    };
    eprintln!(
        "[cc] DRAG_TARGET(browser) '{from}'->'{to}' : css({:.0},{:.0})->({:.0},{:.0})",
        from_point.0, from_point.1, to_point.0, to_point.1
    );
    if cancel.load(Ordering::SeqCst) {
        return target.tag(
            super::super::super::browser::cancelled_before_pointer_effect(
                "vision_to_pointer_handoff",
            ),
        );
    }
    let result = match target.drag(from_point.0, from_point.1, to_point.0, to_point.1, cancel) {
        Ok(()) => json!({
            "ok": true, "via": "browser", "from": from_point.2, "to": to_point.2,
            "from_css": [from_point.0.round(), from_point.1.round()],
            "to_css": [to_point.0.round(), to_point.1.round()],
        }),
        Err(error) => super::super::super::browser::pointer_error_response(error),
    };
    target.tag(result)
}

#[cfg(test)]
mod tests {
    use super::{BrowserVisionTarget, browser_vision_target};
    use crate::overlay::computer_control::controller::world::{
        BrowserWindowIdentity, SurfaceIdentity,
    };
    use crate::overlay::computer_control::uia_task::FrameSource;

    fn browser_window() -> BrowserWindowIdentity {
        BrowserWindowIdentity {
            browser_window_id: 2,
            hwnd: 3,
            pid: 4,
            generation: 5,
        }
    }

    #[test]
    fn source_document_stays_exact_when_foreground_tab_drifts() {
        let source = FrameSource {
            frame_id: 9,
            surface: SurfaceIdentity::Browser {
                tab_id: 73,
                document_id: "doc-9".into(),
                window: browser_window(),
            },
        };
        assert_eq!(
            browser_vision_target(Some(73), Some(&source)).unwrap(),
            Some(BrowserVisionTarget::Exact {
                tab_id: 73,
                document_id: "doc-9".into(),
                window: browser_window(),
            })
        );
        assert!(browser_vision_target(Some(74), Some(&source)).is_err());
    }

    #[test]
    fn native_source_never_falls_through_to_current_browser() {
        let source = FrameSource {
            frame_id: 10,
            surface: SurfaceIdentity::Native {
                hwnd: 5,
                pid: 6,
                generation: 7,
            },
        };
        assert_eq!(browser_vision_target(None, Some(&source)).unwrap(), None);
        assert!(browser_vision_target(None, None).is_err());
    }
}
