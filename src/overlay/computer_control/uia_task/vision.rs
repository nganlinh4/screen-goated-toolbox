//! The aux-vision (VLM) helpers — view reads, point/box location, multi-target
//! mapping, refinement — plus the click-trace + final-review utilities, split
//! out of `uia_task.rs` for the file-size limit. `use super::*` pulls in the
//! shared imports/types; `super::super::` reaches the sibling CC modules.

use super::super::vision_reader::Located;
use super::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

mod browser;
pub(super) use browser::{browser_click, browser_drag, browser_vision_target};
mod candidate_artifacts;
mod frozen_read;
#[cfg(test)]
mod geometry_tests;

pub(super) fn write_artifact(
    kind: &str,
    name: &str,
    bytes: &[u8],
    action: Option<super::super::telemetry::ActionTrace>,
) -> bool {
    frozen_read::write_artifact(kind, name, bytes, action)
}

pub(super) fn persist_candidates(
    request_id: u64,
    input_sha256: &str,
    bundle_sha256: &str,
    attempts: &[super::super::vision_reader::CandidateAttempt],
    action: Option<super::super::telemetry::ActionTrace>,
) -> (String, bool) {
    frozen_read::persist_candidates(request_id, input_sha256, bundle_sha256, attempts, action)
}

/// Longest edge target for the view crop sent to the model (short edge actually).
pub(super) const VIEW_SHORT: u32 = 1024;

/// Short-edge size for the CLEAN crop sent to the aux vision reader. Larger than
/// the Live frame (the reader is not token-capped) so fine detail survives.
pub(super) const VISION_SHORT: u32 = super::super::vision_contract::CONTROL_VISION_SHORT_EDGE;
pub(super) const GROUNDING_SIGNATURE_HALF: i32 = 48;

#[derive(Clone, Debug)]
pub(super) struct VisualLocation {
    pub view: View,
    pub x: f64,
    pub y: f64,
    pub note: Option<String>,
    pub signature: Vec<u8>,
}

impl VisualLocation {
    pub fn screen_point(&self) -> (i32, i32) {
        self.view.to_screen_px(self.x, self.y)
    }
}

fn visual_location(cap: &session::Capture, view: View, point: Located) -> VisualLocation {
    let (x, y) = view.to_screen_px(point.x, point.y);
    VisualLocation {
        view,
        x: point.x,
        y: point.y,
        note: point.note,
        signature: session::target_region_fingerprint(cap, x, y, GROUNDING_SIGNATURE_HALF),
    }
}

fn require_same_view(expected: View, actual: View) -> Result<()> {
    anyhow::ensure!(
        expected == actual,
        "the image region changed while the target was being verified"
    );
    Ok(())
}

/// Read the current view with the aux vision stack (clean crop, no grid overlay).
/// `ctx` is task/intent context for disambiguation. Returns the plain answer.
pub(super) fn read_view(
    view: View,
    question: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<String> {
    read_view_pref(view, question, ctx, cancel, &[])
}

/// [`read_view`] but trying optional `prefer` model ids before the configured
/// image-to-text chain.
pub(super) fn read_view_pref(
    view: View,
    question: &str,
    ctx: &str,
    cancel: &AtomicBool,
    prefer: &[&str],
) -> Result<String> {
    frozen_read::read_plain(view, question, ctx, cancel, prefer)
}

impl Brain {
    /// Stall safety-net — the "merge" planner: ONE grounded vision call that SEES
    /// the screen and proposes the single best NEXT action when the agent is
    /// looping (perception + plan in one shot). Uses the configured vision chain.
    /// None on vision failure; the compact typed postcondition
    /// still reports the unchanged state. Cancellable, so barge-in is immediate.
    pub fn stuck_advice(&self, task: &str, cancel: &AtomicBool) -> Option<String> {
        let view = window_view(self.target.as_deref(), self.whole_screen);
        let trail = if self.recent_actions.is_empty() {
            "(none)".to_string()
        } else {
            self.recent_actions.join("  |  ")
        };
        let q = format!(
            "A computer-control agent is STUCK: it repeated the same action ~3 times and NOTHING on screen changed. \
Task: \"{task}\". Its recent actions: {trail}. \
Look at the screenshot, work out WHY it is stuck, and give the single best NEXT action. \
Be concrete and grounded in what you SEE - name the exact on-screen element, where it is, and its state \
(greyed-out, behind a dialog, off-screen, wrong tab, or the task is already done). \
Answer in ONE or TWO sentences as a direct instruction to the agent. No preamble."
        );
        let advice = read_view_pref(view, &q, &format!("task: {task}"), cancel, &[])
            .ok()?
            .trim()
            .to_string();
        (!advice.is_empty()).then_some(advice)
    }
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
    std::thread::spawn(super::super::session_retention::inherit(move || {
        let _ = tx.send(work());
    }));
    loop {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("cancelled by user");
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("vision worker disconnected")
            }
        }
    }
}

/// Run one blocking provider operation with both user cancellation and a hard
/// wall-clock budget. The child flag is passed into transports that can abort
/// cooperatively; callers still return on time if a transport is stuck before
/// it can observe that flag.
pub(super) fn run_cancellable_with_timeout<T, F>(
    cancel: &AtomicBool,
    timeout: Duration,
    work: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let child_cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&child_cancel);
    std::thread::spawn(super::super::session_retention::inherit(move || {
        let _ = tx.send(work(worker_cancel));
    }));
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) {
            child_cancel.store(true, Ordering::SeqCst);
            anyhow::bail!("cancelled by user");
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            child_cancel.store(true, Ordering::SeqCst);
            anyhow::bail!("vision request timed out after {} ms", timeout.as_millis());
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("vision worker disconnected")
            }
        }
    }
}

/// Wall-clock budget for one grounding call site: two providers at their
/// per-request cap, after which the single action worker is released.
const GROUNDING_BUDGET: Duration = Duration::from_secs(60);

/// Run a grounding call with user cancellation and [`GROUNDING_BUDGET`]; the
/// child flag aborts the in-flight provider request on either outcome.
pub(super) fn run_grounding<T, F>(cancel: &AtomicBool, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
{
    run_cancellable_with_timeout(cancel, GROUNDING_BUDGET, work)
}

/// Ask the aux vision stack for the click point of `description`, returned as
/// 0-1000 over `view` (+ what's there). DEFAULT: a SINGLE point call (fast, and
/// point-based so it's accurate for normal targets). `CC_LOCATE_MODE=refine` adds
/// a second zoomed pass for tiny adjacent cells (2x the latency); `=box` uses one
/// bounding-box call.
pub(super) fn locate_in_view(
    view: View,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<VisualLocation> {
    let cap = session::capture_virtual()?;
    let (jpeg, view) = session::encode_view(&cap, view, VISION_SHORT, None, None, None)?;
    let located = match std::env::var("CC_LOCATE_MODE").as_deref() {
        Ok("refine") => refine_in_view(&cap, view, &jpeg, description, ctx, cancel),
        Ok("box") => {
            let (j, d, c) = (jpeg.clone(), description.to_string(), ctx.to_string());
            match run_grounding(cancel, move |child| {
                super::super::vision_reader::locate_box(&j, &d, &c, child)
            }) {
                Ok(p) => Ok(p),
                Err(_) => {
                    let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
                    run_grounding(cancel, move |child| {
                        super::super::vision_reader::locate_point(&j, &d, &c, child)
                    })
                }
            }
        }
        // DEFAULT: one point call - half the latency of refine, accurate for
        // normal UI; opt into refine for tiny adjacent cells (game boards).
        _ => {
            let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
            run_grounding(cancel, move |child| {
                super::super::vision_reader::locate_point(&j, &d, &c, child)
            })
        }
    }?;
    // Re-capture after the potentially slow locate call. This both detects a
    // stale/moving UI and verifies the exact proposed point on a marked crop.
    let fresh = session::capture_virtual()?;
    let (fresh_jpeg, fresh_view) =
        session::encode_view(&fresh, view, VISION_SHORT, None, None, None)?;
    require_same_view(view, fresh_view)?;
    let verified = verify_located(&fresh_jpeg, located, description, ctx, cancel)?;
    Ok(visual_location(&fresh, view, verified))
}

/// Ask the aux vision stack to map EVERY target matching `description` to a list
/// of points (0-1000 over `view`), cancellable. Used to build reusable click
/// anchors in one call.
pub(super) fn map_in_view(
    view: View,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<Vec<VisualLocation>> {
    let cap = session::capture_virtual()?;
    let (jpeg, view) = session::encode_view(&cap, view, VISION_SHORT, None, None, None)?;
    let (d, c) = (description.to_string(), ctx.to_string());
    let points = run_grounding(cancel, move |child| {
        super::super::vision_reader::locate_points(&jpeg, &d, &c, child)
    })?;
    Ok(points
        .into_iter()
        .map(|point| visual_location(&cap, view, point))
        .collect())
}

pub(super) fn drag_in_view(
    view: View,
    from_description: &str,
    to_description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<(VisualLocation, VisualLocation)> {
    let capture = session::capture_virtual()?;
    let (jpeg, view) = session::encode_view(&capture, view, VISION_SHORT, None, None, None)?;
    let (from_owned, to_owned, context_owned) = (
        from_description.to_string(),
        to_description.to_string(),
        ctx.to_string(),
    );
    let (from, to) = run_grounding(cancel, move |child| {
        super::super::vision_reader::locate_drag_points(
            &jpeg,
            &from_owned,
            &to_owned,
            &context_owned,
            child,
        )
    })?;
    let fresh = session::capture_virtual()?;
    let (fresh_jpeg, fresh_view) =
        session::encode_view(&fresh, view, VISION_SHORT, None, None, None)?;
    require_same_view(view, fresh_view)?;
    let from = verify_located(&fresh_jpeg, from, from_description, ctx, cancel)?;
    let to = verify_located(&fresh_jpeg, to, to_description, ctx, cancel)?;
    let convert = |point| visual_location(&fresh, view, point);
    Ok((convert(from), convert(to)))
}

pub(super) fn revalidate_visual_locations(locations: &[&VisualLocation]) -> Result<()> {
    let fresh = session::capture_virtual()?;
    validate_visual_locations(&fresh, locations)
}

fn validate_visual_locations(
    fresh: &session::Capture,
    locations: &[&VisualLocation],
) -> Result<()> {
    let bounds = fresh.bounds()?;
    for location in locations {
        require_same_view(location.view, location.view.clamped_to(bounds))?;
        let (screen_x, screen_y) = location.screen_point();
        let current =
            session::target_region_fingerprint(fresh, screen_x, screen_y, GROUNDING_SIGNATURE_HALF);
        if !session::target_fingerprint_matches(&location.signature, &current) {
            anyhow::bail!("the target pixels changed before input dispatch");
        }
    }
    Ok(())
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
        let (j, d, c) = (
            coarse_jpeg.to_vec(),
            description.to_string(),
            ctx.to_string(),
        );
        run_grounding(cancel, move |child| {
            super::super::vision_reader::locate_point(&j, &d, &c, child)
        })?
    };
    let (csx, csy) = view.to_screen_px(coarse.x, coarse.y);
    let zw = (view.w / 4).max(160);
    let zh = (view.h / 4).max(120);
    let zoom = View {
        x: csx.saturating_sub(zw / 2),
        y: csy.saturating_sub(zh / 2),
        w: zw,
        h: zh,
    }
    .clamped_to(view);
    let Ok((fine_jpeg, shown)) = session::encode_view(cap, zoom, VISION_SHORT, None, None, None)
    else {
        return Ok(coarse);
    };
    // The fine pass keeps the same fail-closed grounding chain. A zoomed crop
    // improves localization without granting a general image model click authority.
    let fine = {
        let (d, c) = (description.to_string(), ctx.to_string());
        run_grounding(cancel, move |child| {
            super::super::vision_reader::locate_point(&fine_jpeg, &d, &c, child)
        })
    };
    match fine {
        Ok(f) => {
            let (fsx, fsy) = shown.to_screen_px(f.x, f.y);
            let (mx, my) = view.screen_to_norm(fsx, fsy);
            cc_diagnostic!(
                "[cc] locate refine: coarse({:.0},{:.0}) -> fine({mx:.0},{my:.0})",
                coarse.x,
                coarse.y
            );
            Ok(Located {
                x: mx,
                y: my,
                note: f.note.or(coarse.note),
            })
        }
        Err(_) => Ok(coarse),
    }
}

// ── Browser-pipeline variants (used when browser::input_active()) ─────────────
//
// Same vision-locate, but over a crisp CDP page screenshot instead of the OS
// frame, and the action goes through the browser's TRUSTED input (CDP) instead of
// SendInput. This is what lets the agent operate <canvas>/WebGL web games and
// cross-origin iframes that ignore synthetic OS mouse events — with better pixel
// precision too (full-res page image, exact CSS-px coordinates, no chrome/DPR math).

/// True if every token in a `key_combination` is a scroll/navigation key — those
/// are legitimately repeated (paging through a feed), so the stuck detector skips
/// them. A combo with a non-nav key (e.g. Ctrl+C) is not navigation.
pub(super) fn is_nav_keys(keys: &str) -> bool {
    const NAV: &[&str] = &[
        "up", "down", "left", "right", "pageup", "pagedown", "home", "end", "space", "tab",
        "scroll",
    ];
    let toks: Vec<String> = keys
        .split(['+', ' '])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();
    !toks.is_empty() && toks.iter().all(|t| NAV.contains(&t.as_str()))
}

/// Append one correlated click record to this session's `clicks.jsonl`.
pub(super) fn append_click(dir: &str, action: super::super::telemetry::ActionTrace, fields: Value) {
    use super::super::telemetry;
    use std::io::Write;
    let click_id = telemetry::next_artifact_id();
    let mut record = telemetry::artifact_record("click", click_id, Some(action), fields);
    record["coordinate_spaces"] = json!({
        "view_norm": "0..1000 relative to view_rect",
        "screen_px": "virtual-desktop pixels",
        "view_rect": "screen pixels [x,y,width,height]",
    });
    let path = std::path::Path::new(dir).join("clicks.jsonl");
    let result = super::super::session_retention::persist(|| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| writeln!(file, "{record}"))
    });
    match result {
        Some(Ok(())) => telemetry::event_for_action(
            "click_recorded",
            "artifact",
            super::super::telemetry::Privacy::Safe,
            action,
            json!({
                "click_id": click_id,
                "artifact_path": "clicks.jsonl",
                "step": record.get("step"),
                "kind": record.get("kind"),
                "screen_px": record.get("screen_px"),
            }),
        ),
        Some(Err(error)) => telemetry::artifact_write_failed("click", &path, Some(action), &error),
        None => {}
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[test]
    fn deadline_returns_promptly_and_notifies_the_provider_worker() {
        let outer = AtomicBool::new(false);
        let child_seen = Arc::new(AtomicBool::new(false));
        let worker_seen = Arc::clone(&child_seen);
        let started = Instant::now();
        let result =
            run_cancellable_with_timeout(&outer, Duration::from_millis(30), move |child_cancel| {
                while !child_cancel.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
                worker_seen.store(true, Ordering::SeqCst);
                Ok(())
            });
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_millis(250));
        for _ in 0..50 {
            if child_seen.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("provider worker did not observe deadline cancellation");
    }
}
