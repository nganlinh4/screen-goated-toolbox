//! The aux-vision (VLM) helpers — view reads, point/box location, multi-target
//! mapping, refinement — plus the click-trace + final-review utilities, split
//! out of `uia_task.rs` for the file-size limit. `use super::*` pulls in the
//! shared imports/types; `super::super::` reaches the sibling CC modules.

use super::*;
use super::super::vision_reader::Located;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

/// Longest edge target for the view crop sent to the model (short edge actually).
pub(super) const VIEW_SHORT: u32 = 1024;

/// Short-edge size for the CLEAN crop sent to the aux vision reader. Larger than
/// the Live frame (the reader is not token-capped) so fine detail survives.
pub(super) const VISION_SHORT: u32 = 1600;

/// Read the current view with the aux vision stack (clean crop, no grid overlay).
/// `ctx` is task/intent context for disambiguation. Returns the plain answer.
pub(super) fn read_view(view: View, question: &str, ctx: &str, cancel: &AtomicBool) -> Result<String> {
    let cap = session::capture_virtual()?;
    let (jpeg, _shown) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    let (q, c) = (question.to_string(), ctx.to_string());
    run_cancellable(cancel, move || super::super::vision_reader::read_image(&jpeg, &q, &c))
}

/// Run a (slow, blocking) vision call on a worker thread while polling `cancel`
/// every 50ms. A barge-in returns immediately ("cancelled") instead of blocking
/// the agent on a 15-25s HTTP round-trip; the abandoned call finishes in the
/// background and its result is dropped. Capture/encode stay on the caller (fast).
pub(super) fn run_cancellable<T, F>(cancel: &AtomicBool, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    loop {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("cancelled by user");
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("vision worker disconnected"),
        }
    }
}

/// Ask the aux vision stack for the click point of `description`, returned as
/// 0-1000 over `view` (+ what's there). DEFAULT: a SINGLE point call (fast, and
/// point-based so it's accurate for normal targets). `CC_LOCATE_MODE=refine` adds
/// a second zoomed pass for tiny adjacent cells (2x the latency); `=box` uses one
/// bounding-box call.
pub(super) fn locate_in_view(view: View, description: &str, ctx: &str, cancel: &AtomicBool) -> Result<Located> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    match std::env::var("CC_LOCATE_MODE").as_deref() {
        Ok("refine") => refine_in_view(&cap, view, &jpeg, description, ctx, cancel),
        Ok("box") => {
            let (j, d, c) = (jpeg.clone(), description.to_string(), ctx.to_string());
            match run_cancellable(cancel, move || super::super::vision_reader::locate_box(&j, &d, &c)) {
                Ok(p) => Ok(p),
                Err(_) => {
                    let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
                    run_cancellable(cancel, move || super::super::vision_reader::locate_point(&j, &d, &c))
                }
            }
        }
        // DEFAULT: one point call - half the latency of refine, accurate for
        // normal UI; opt into refine for tiny adjacent cells (game boards).
        _ => {
            let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
            run_cancellable(cancel, move || super::super::vision_reader::locate_point(&j, &d, &c))
        }
    }
}

/// Ask the aux vision stack to map EVERY target matching `description` to a list
/// of points (0-1000 over `view`), cancellable. Used to build reusable click
/// anchors in one call.
pub(super) fn map_in_view(view: View, description: &str, ctx: &str, cancel: &AtomicBool) -> Result<Vec<Located>> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    let (d, c) = (description.to_string(), ctx.to_string());
    run_cancellable(cancel, move || super::super::vision_reader::locate_points(&jpeg, &d, &c))
}

/// Two-call coarse-to-fine locate: point over the whole view, then ZOOM a box
/// around it and point again so the target fills the frame.
pub(super) fn refine_in_view(
    cap: &session::Capture,
    view: View,
    coarse_jpeg: &[u8],
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<Located> {
    let coarse = {
        let (j, d, c) = (coarse_jpeg.to_vec(), description.to_string(), ctx.to_string());
        run_cancellable(cancel, move || super::super::vision_reader::locate_point(&j, &d, &c))?
    };
    let (csx, csy) = view.to_screen_px(coarse.x, coarse.y);
    let zw = (view.w / 4).max(160);
    let zh = (view.h / 4).max(120);
    let zoom = View { x: csx - zw / 2, y: csy - zh / 2, w: zw, h: zh };
    let Ok((fine_jpeg, shown)) = session::encode_view(cap, zoom, VISION_SHORT, None, None) else {
        return Ok(coarse);
    };
    // The fine pass is easy localization (target fills the zoomed crop), so an
    // optional faster model (CC_VISION_FINE_MODEL) can do it — falling back to
    // the accurate default if it fails. Stateless; never loses correctness.
    let fine = {
        let (d, c) = (description.to_string(), ctx.to_string());
        let fine_model = std::env::var("CC_VISION_FINE_MODEL").ok().filter(|m| !m.trim().is_empty());
        run_cancellable(cancel, move || match fine_model {
            Some(m) => super::super::vision_reader::locate_point_with(&fine_jpeg, &d, m.trim(), &c),
            None => super::super::vision_reader::locate_point(&fine_jpeg, &d, &c),
        })
    };
    match fine {
        Ok(f) => {
            let (fsx, fsy) = shown.to_screen_px(f.x, f.y);
            let mx = ((fsx - view.x) as f64 / view.w.max(1) as f64 * 1000.0).clamp(0.0, 1000.0);
            let my = ((fsy - view.y) as f64 / view.h.max(1) as f64 * 1000.0).clamp(0.0, 1000.0);
            eprintln!("[cc] locate refine: coarse({:.0},{:.0}) -> fine({mx:.0},{my:.0})", coarse.x, coarse.y);
            Ok(Located { x: mx, y: my, note: f.note.or(coarse.note) })
        }
        Err(_) => Ok(coarse),
    }
}

/// True if every token in a `key_combination` is a scroll/navigation key — those
/// are legitimately repeated (paging through a feed), so the stuck detector skips
/// them. A combo with a non-nav key (e.g. Ctrl+C) is not navigation.
pub(super) fn is_nav_keys(keys: &str) -> bool {
    const NAV: &[&str] = &[
        "up", "down", "left", "right", "pageup", "pagedown", "home", "end", "space", "tab", "scroll",
    ];
    let toks: Vec<String> = keys
        .split(['+', ' '])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();
    !toks.is_empty() && toks.iter().all(|t| NAV.contains(&t.as_str()))
}

/// Append one click record to `{dir}/clicks.jsonl` (the click-accuracy trace).
pub(super) fn append_click(dir: &str, rec: Value) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{dir}/clicks.jsonl"))
    {
        let _ = writeln!(f, "{rec}");
    }
}

/// Every recorded click's screen px, for the cumulative final overlay.
pub(super) fn read_click_points(dir: &str) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    if let Ok(s) = std::fs::read_to_string(format!("{dir}/clicks.jsonl")) {
        for line in s.lines() {
            if let Ok(v) = serde_json::from_str::<Value>(line)
                && let Some(p) = v.get("screen_px").and_then(|x| x.as_array())
                && p.len() == 2
            {
                out.push((
                    p[0].as_i64().unwrap_or(0) as i32,
                    p[1].as_i64().unwrap_or(0) as i32,
                ));
            }
        }
    }
    out
}

/// After the task ends, do a final high-res vision reading of the result and
/// save a frame with EVERY click point marked — so we can tell whether a wrong
/// outcome was a harness mis-click or a model decision.
pub(super) fn final_review(dir: &str, target: Option<&str>, note: &str) {
    let view = window_view(target);
    let reading = read_view(
        view,
        "Describe the final on-screen state in detail. If this is a game, state the exact result \
(win / lose / draw) and the full final board.",
        "",
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|e| format!("(vision read failed: {e})"));
    let _ = std::fs::write(
        format!("{dir}/final.txt"),
        format!("NOTE: {note}\n\nFINAL VISION READING:\n{reading}\n"),
    );
    eprintln!("[cc] FINAL REVIEW ({note}):\n{reading}");

    if let Ok(cap) = session::capture_virtual()
        && let Ok((jpeg, clamped)) = session::encode_view(&cap, view, VISION_SHORT, None, None)
        && let Ok(img) = image::load_from_memory(&jpeg)
    {
        let mut rgb = img.to_rgb8();
        for (sx, sy) in read_click_points(dir) {
            let fx = ((sx - clamped.x) as f64 / clamped.w.max(1) as f64 * rgb.width() as f64).round() as i32;
            let fy = ((sy - clamped.y) as f64 / clamped.h.max(1) as f64 * rgb.height() as f64).round() as i32;
            super::super::grid::draw_click_marker(&mut rgb, fx, fy);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        if image::DynamicImage::ImageRgb8(rgb)
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .is_ok()
        {
            let _ = std::fs::write(format!("{dir}/final-clicks.jpg"), buf.into_inner());
        }
    }
}
