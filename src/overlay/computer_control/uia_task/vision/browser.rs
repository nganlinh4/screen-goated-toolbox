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

struct BrowserVisualLocation {
    x: f64,
    y: f64,
    note: Option<String>,
    signature: Vec<u8>,
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
) -> Result<BrowserVisualLocation> {
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
    let x = located.x / 1000.0 * width;
    let y = located.y / 1000.0 * height;
    Ok(BrowserVisualLocation {
        x,
        y,
        note: located.note,
        signature: jpeg_region_fingerprint(&fresh_jpeg, x, y)?,
    })
}

pub(in crate::overlay::computer_control::uia_task) fn browser_click(
    target: BrowserVisionTarget,
    description: &str,
    right: bool,
    ctx: &str,
    cancel: &AtomicBool,
) -> Value {
    let result = match locate_css(&target, description, ctx, cancel) {
        Ok(located) => {
            eprintln!(
                "[cc] CLICK_TARGET(browser) '{description}' -> css({:.0},{:.0}) saw={:?}",
                located.x, located.y, located.note
            );
            if cancel.load(Ordering::SeqCst) {
                return target.tag(
                    super::super::super::browser::cancelled_before_pointer_effect(
                        "vision_to_pointer_handoff",
                    ),
                );
            }
            if let Err(error) = revalidate_browser_locations(&target, &[&located]) {
                return target.tag(json!({
                    "ok": false,
                    "code": "ERR_STALE_VISUAL_TARGET",
                    "effect_may_have_occurred": false,
                    "error": error.to_string(),
                }));
            }
            match target.click(located.x, located.y, right, cancel) {
                Ok(()) => json!({
                    "ok": true, "via": "browser",
                    "css_px": [located.x.round(), located.y.round()],
                    "saw_at_target": located.note,
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
    let (from_point, to_point) = match locate_drag_css(&target, from, to, ctx, cancel) {
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
                "error": format!("could not locate drag endpoints: {error}"),
            }));
        }
    };
    eprintln!(
        "[cc] DRAG_TARGET(browser) '{from}'->'{to}' : css({:.0},{:.0})->({:.0},{:.0})",
        from_point.x, from_point.y, to_point.x, to_point.y
    );
    if cancel.load(Ordering::SeqCst) {
        return target.tag(
            super::super::super::browser::cancelled_before_pointer_effect(
                "vision_to_pointer_handoff",
            ),
        );
    }
    if let Err(error) = revalidate_browser_locations(&target, &[&from_point, &to_point]) {
        return target.tag(json!({
            "ok": false,
            "code": "ERR_STALE_VISUAL_TARGET",
            "effect_may_have_occurred": false,
            "error": error.to_string(),
        }));
    }
    let result = match target.drag(from_point.x, from_point.y, to_point.x, to_point.y, cancel) {
        Ok(()) => json!({
            "ok": true, "via": "browser", "from": from_point.note, "to": to_point.note,
            "from_css": [from_point.x.round(), from_point.y.round()],
            "to_css": [to_point.x.round(), to_point.y.round()],
        }),
        Err(error) => super::super::super::browser::pointer_error_response(error),
    };
    target.tag(result)
}

fn locate_drag_css(
    target: &BrowserVisionTarget,
    from: &str,
    to: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<(BrowserVisualLocation, BrowserVisualLocation)> {
    let (jpeg, width, height) = target.shot()?;
    let (from_owned, to_owned, context_owned) = (from.to_string(), to.to_string(), ctx.to_string());
    let (from_point, to_point) = run_cancellable(cancel, move || {
        super::super::super::vision_reader::locate_drag_points(
            &jpeg,
            &from_owned,
            &to_owned,
            &context_owned,
        )
    })?;
    let (fresh, fresh_width, fresh_height) = target.shot()?;
    if (fresh_width - width).abs() > f64::EPSILON || (fresh_height - height).abs() > f64::EPSILON {
        anyhow::bail!("browser viewport changed while locating drag endpoints");
    }
    let from_point = verify_located(&fresh, from_point, from, ctx, cancel)?;
    let to_point = verify_located(&fresh, to_point, to, ctx, cancel)?;
    let convert = |point: super::super::super::vision_reader::Located| -> Result<_> {
        let x = point.x / 1000.0 * width;
        let y = point.y / 1000.0 * height;
        Ok(BrowserVisualLocation {
            x,
            y,
            note: point.note,
            signature: jpeg_region_fingerprint(&fresh, x, y)?,
        })
    };
    Ok((convert(from_point)?, convert(to_point)?))
}

fn revalidate_browser_locations(
    target: &BrowserVisionTarget,
    locations: &[&BrowserVisualLocation],
) -> Result<()> {
    let (fresh, _, _) = target.shot()?;
    for location in locations {
        let current = jpeg_region_fingerprint(&fresh, location.x, location.y)?;
        if !super::super::super::session::target_fingerprint_matches(&location.signature, &current)
        {
            anyhow::bail!("the browser target pixels changed before input dispatch");
        }
    }
    Ok(())
}

fn jpeg_region_fingerprint(jpeg: &[u8], x: f64, y: f64) -> Result<Vec<u8>> {
    let image = image::load_from_memory(jpeg)?.to_rgb8();
    let half = super::super::GROUNDING_SIGNATURE_HALF.max(1) as u32;
    let center_x = x.round().clamp(0.0, image.width().saturating_sub(1) as f64) as u32;
    let center_y = y
        .round()
        .clamp(0.0, image.height().saturating_sub(1) as f64) as u32;
    let left = center_x.saturating_sub(half);
    let top = center_y.saturating_sub(half);
    let right = center_x
        .saturating_add(half)
        .min(image.width())
        .max(left + 1);
    let bottom = center_y
        .saturating_add(half)
        .min(image.height())
        .max(top + 1);
    let crop = image::imageops::crop_imm(&image, left, top, right - left, bottom - top).to_image();
    let small = image::imageops::resize(&crop, 32, 32, image::imageops::FilterType::Triangle);
    Ok(small.pixels().flat_map(|pixel| pixel.0).collect())
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
