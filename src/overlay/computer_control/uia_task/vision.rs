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
pub(super) const VISION_SHORT: u32 = 1600;

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

    pub fn final_review(&self, task: &str, note: &str) {
        final_review(&self.dir, self.target.as_deref(), task, note);
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
    std::thread::spawn(move || {
        let _ = tx.send(work(worker_cancel));
    });
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
) -> Result<Located> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None, None)?;
    let located = match std::env::var("CC_LOCATE_MODE").as_deref() {
        Ok("refine") => refine_in_view(&cap, view, &jpeg, description, ctx, cancel),
        Ok("box") => {
            let (j, d, c) = (jpeg.clone(), description.to_string(), ctx.to_string());
            match run_cancellable(cancel, move || {
                super::super::vision_reader::locate_box(&j, &d, &c)
            }) {
                Ok(p) => Ok(p),
                Err(_) => {
                    let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
                    run_cancellable(cancel, move || {
                        super::super::vision_reader::locate_point(&j, &d, &c)
                    })
                }
            }
        }
        // DEFAULT: one point call - half the latency of refine, accurate for
        // normal UI; opt into refine for tiny adjacent cells (game boards).
        _ => {
            let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
            run_cancellable(cancel, move || {
                super::super::vision_reader::locate_point(&j, &d, &c)
            })
        }
    }?;
    // Re-capture after the potentially slow locate call. This both detects a
    // stale/moving UI and verifies the exact proposed point on a marked crop.
    let fresh = session::capture_virtual()?;
    let (fresh_jpeg, _) = session::encode_view(&fresh, view, VISION_SHORT, None, None, None)?;
    verify_located(&fresh_jpeg, located, description, ctx, cancel)
}

/// Ask the aux vision stack to map EVERY target matching `description` to a list
/// of points (0-1000 over `view`), cancellable. Used to build reusable click
/// anchors in one call.
pub(super) fn map_in_view(
    view: View,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<Vec<Located>> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None, None)?;
    let (d, c) = (description.to_string(), ctx.to_string());
    run_cancellable(cancel, move || {
        super::super::vision_reader::locate_points(&jpeg, &d, &c)
    })
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
        run_cancellable(cancel, move || {
            super::super::vision_reader::locate_point(&j, &d, &c)
        })?
    };
    let (csx, csy) = view.to_screen_px(coarse.x, coarse.y);
    let zw = (view.w / 4).max(160);
    let zh = (view.h / 4).max(120);
    let zoom = View {
        x: csx - zw / 2,
        y: csy - zh / 2,
        w: zw,
        h: zh,
    };
    let Ok((fine_jpeg, shown)) = session::encode_view(cap, zoom, VISION_SHORT, None, None, None)
    else {
        return Ok(coarse);
    };
    // The fine pass is easy localization (target fills the zoomed crop), so an
    // optional faster model (CC_VISION_FINE_MODEL) can do it — falling back to
    // the accurate default if it fails. Stateless; never loses correctness.
    let fine = {
        let (d, c) = (description.to_string(), ctx.to_string());
        let fine_model = std::env::var("CC_VISION_FINE_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty());
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
            eprintln!(
                "[cc] locate refine: coarse({:.0},{:.0}) -> fine({mx:.0},{my:.0})",
                coarse.x, coarse.y
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
    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| writeln!(file, "{record}"));
    match result {
        Ok(()) => telemetry::event_for_action(
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
        Err(error) => telemetry::artifact_write_failed("click", &path, Some(action), &error),
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
