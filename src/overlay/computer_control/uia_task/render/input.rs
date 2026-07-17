//! Native pointer/raw-input helpers bound to a model-visible frame surface.

use super::super::*;

pub(in crate::overlay::computer_control::uia_task) struct InputContext<'a> {
    pub dry: bool,
    pub profile: &'a HumanProfile,
    pub cancel: &'a AtomicBool,
    pub target: Option<&'a str>,
    pub source: Option<&'a FrameSource>,
}

pub(in crate::overlay::computer_control::uia_task) fn click_screen(
    sx: i32,
    sy: i32,
    button: &str,
    input: InputContext<'_>,
) -> Value {
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (sx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (sy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if input.dry {
        return json!({"ok": true, "note": "dry", "screen_px": [sx, sy], "button": button});
    }
    let args = match guarded_input_args(
        json!({"x": nx, "y": ny, "button": button, "uncertain": true}),
        input.target,
        input.source,
    ) {
        Ok(args) => args,
        Err(error) => return json!({"ok": false, "error": error.to_string()}),
    };
    executor::execute_ex("click", &args, input.profile, input.cancel)
}

pub(in crate::overlay::computer_control::uia_task) fn pointer_result(
    input_result: Value,
    view: View,
    view_norm: (f64, f64),
    screen_px: (i32, i32),
    extra: Value,
) -> Value {
    let ok = input_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut fields = match extra {
        Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };
    fields.insert("ok".to_string(), json!(ok));
    fields.insert("view_norm".to_string(), json!([view_norm.0, view_norm.1]));
    fields.insert("screen_px".to_string(), json!([screen_px.0, screen_px.1]));
    fields.insert(
        "view_rect".to_string(),
        json!([view.x, view.y, view.w, view.h]),
    );
    fields.insert(
        "coordinate_spaces".to_string(),
        json!({
            "view_norm": "0..1000 relative to view_rect",
            "screen_px": "virtual-desktop pixels",
            "view_rect": "screen pixels [x,y,width,height]",
        }),
    );
    fields.insert("input_result".to_string(), input_result);
    Value::Object(fields)
}

pub(in crate::overlay::computer_control::uia_task) fn screen_to_view_norm(
    view: View,
    sx: i32,
    sy: i32,
) -> (f64, f64) {
    (
        (sx - view.x) as f64 / view.w.max(1) as f64 * 1000.0,
        (sy - view.y) as f64 / view.h.max(1) as f64 * 1000.0,
    )
}

pub(in crate::overlay::computer_control::uia_task) fn point_screen(
    sx: i32,
    sy: i32,
    dwell_ms: u64,
    input: InputContext<'_>,
) -> Value {
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (sx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (sy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if input.dry {
        return json!({"ok": true, "note": "dry", "screen_px": [sx, sy]});
    }
    let args = match guarded_input_args(
        json!({"x": nx, "y": ny, "dwell_ms": dwell_ms}),
        input.target,
        input.source,
    ) {
        Ok(args) => args,
        Err(error) => return json!({"ok": false, "error": error.to_string()}),
    };
    executor::execute_ex("point", &args, input.profile, input.cancel)
}

pub(in crate::overlay::computer_control::uia_task) fn drag_screen(
    from: (i32, i32),
    to: (i32, i32),
    input: InputContext<'_>,
) -> Value {
    let ((fx, fy), (tx, ty)) = (from, to);
    if input.dry {
        return json!({"ok": true, "note": "dry", "from_px": [fx, fy], "to_px": [tx, ty]});
    }
    let (fnx, fny) = executor::screen_to_norm(fx, fy);
    let (tnx, tny) = executor::screen_to_norm(tx, ty);
    let args = match guarded_input_args(
        json!({"x": fnx, "y": fny, "dest_x": tnx, "dest_y": tny}),
        input.target,
        input.source,
    ) {
        Ok(args) => args,
        Err(error) => return json!({"ok": false, "error": error.to_string()}),
    };
    executor::execute_ex("drag", &args, input.profile, input.cancel)
}

pub(in crate::overlay::computer_control::uia_task) fn guarded_input_args(
    mut args: Value,
    target: Option<&str>,
    source: Option<&FrameSource>,
) -> Result<Value> {
    let source =
        source.ok_or_else(|| anyhow::anyhow!("model-visible source frame is unavailable"))?;
    if source.native_identity().is_some()
        && let Some(target) = target
        && !uia::raise_window(target)?
    {
        anyhow::bail!("the pinned input target could not become foreground");
    }
    args["expected_input_target"] = source.input_guard();
    Ok(args)
}

pub(in crate::overlay::computer_control::uia_task) fn guarded_direct_input_args(
    name: &str,
    args: Value,
    target: Option<&str>,
    source: Option<&FrameSource>,
    refocus_required: bool,
) -> Result<Value> {
    if !matches!(name, "type_text" | "key_combination") {
        return guarded_input_args(args, target, source);
    }
    if refocus_required {
        anyhow::bail!(
            "raw keyboard target is stale after a failed window selection; successfully focus the intended window first"
        );
    }
    guarded_keyboard_args(args, target, source)
}

fn guarded_keyboard_args(
    mut args: Value,
    pinned_target: Option<&str>,
    source: Option<&FrameSource>,
) -> Result<Value> {
    let source =
        source.ok_or_else(|| anyhow::anyhow!("model-visible source frame is unavailable"))?;
    let requested = args
        .get("target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .ok_or_else(|| anyhow::anyhow!("raw keyboard input requires a stable window target"))?;
    let stable = uia::stable_window_target(requested)?;
    ensure_keyboard_window(source, uia::window_identity(&stable)?)?;
    if !uia::raise_window(&stable)? {
        anyhow::bail!("the explicit keyboard target could not become foreground");
    }
    args["target"] = Value::String(stable);
    guarded_input_args(args, pinned_target, Some(source))
}

fn ensure_keyboard_window(source: &FrameSource, requested: (u64, u64)) -> Result<()> {
    if source.window_identity() != requested {
        anyhow::bail!(
            "explicit keyboard target does not match the exact model-visible window identity"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_keyboard_requires_refocus_and_an_explicit_target() {
        let source = FrameSource::native(1, (11, 12, 13));
        assert!(
            guarded_direct_input_args(
                "type_text",
                json!({"text": "value"}),
                None,
                Some(&source),
                true,
            )
            .unwrap_err()
            .to_string()
            .contains("stale")
        );
        assert!(
            guarded_direct_input_args(
                "type_text",
                json!({"text": "value"}),
                None,
                Some(&source),
                false,
            )
            .unwrap_err()
            .to_string()
            .contains("stable window target")
        );
    }

    #[test]
    fn keyboard_window_identity_must_match_the_visible_frame() {
        let source = FrameSource::native(1, (11, 12, 13));
        ensure_keyboard_window(&source, (11, 12)).unwrap();
        assert!(ensure_keyboard_window(&source, (11, 99)).is_err());
    }
}
