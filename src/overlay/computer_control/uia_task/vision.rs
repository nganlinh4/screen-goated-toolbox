//! The aux-vision (VLM) helpers — view reads, point/box location, multi-target
//! mapping, refinement — plus the click-trace + final-review utilities, split
//! out of `uia_task.rs` for the file-size limit. `use super::*` pulls in the
//! shared imports/types; `super::super::` reaches the sibling CC modules.

use super::super::vision_reader::Located;
use super::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

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

/// [`read_view`] but trying `prefer` model ids first — the stall planner prefers
/// the benchmark-winning 2.5 vision models before the standard chain.
pub(super) fn read_view_pref(
    view: View,
    question: &str,
    ctx: &str,
    cancel: &AtomicBool,
    prefer: &[&str],
) -> Result<String> {
    use super::super::telemetry::{self, Privacy};
    let request_id = telemetry::next_artifact_id();
    let started = Instant::now();
    let cap = session::capture_virtual().inspect_err(|error| {
        telemetry::typed_error(
            "ERR_VISION_CAPTURE_FAILED",
            "vision",
            "failed to capture the exact input for an auxiliary vision request",
            json!({"request_id": request_id, "error": error.to_string()}),
        );
    })?;
    let (jpeg, shown) =
        session::encode_view(&cap, view, VISION_SHORT, None, None).inspect_err(|error| {
            telemetry::typed_error(
                "ERR_VISION_ENCODE_FAILED",
                "vision",
                "failed to encode the auxiliary vision input",
                json!({"request_id": request_id, "error": error.to_string()}),
            );
        })?;
    let artifact_name = format!("vision-input-{request_id:06}.jpg");
    let artifact_path = telemetry::trace_dir().join(&artifact_name);
    let artifact_write_ok = match std::fs::write(&artifact_path, &jpeg) {
        Ok(()) => true,
        Err(error) => {
            telemetry::artifact_write_failed("vision_input", &artifact_path, None, &error);
            false
        }
    };
    telemetry::event(
        "vision_request",
        "vision",
        Privacy::UserText,
        json!({
            "request_id": request_id,
            "question_preview": question.chars().take(200).collect::<String>(),
            "context_preview": ctx.chars().take(200).collect::<String>(),
            "preferred_models": prefer,
            "byte_count": jpeg.len(),
            "view": [shown.x, shown.y, shown.w, shown.h],
            "artifact_path": artifact_name,
            "artifact_write_ok": artifact_write_ok,
        }),
    );
    let (q, c) = (question.to_string(), ctx.to_string());
    let prefer: Vec<String> = prefer.iter().map(|s| s.to_string()).collect();
    let result = run_cancellable(cancel, move || {
        let p: Vec<&str> = prefer.iter().map(String::as_str).collect();
        super::super::vision_reader::read_image_pref(&jpeg, &q, &c, &p)
    });
    telemetry::event(
        "vision_result",
        "vision",
        Privacy::UserText,
        json!({
            "request_id": request_id,
            "ok": result.is_ok(),
            "duration_ms": started.elapsed().as_millis(),
            "response_preview": result.as_ref().ok().map(|text| text.chars().take(300).collect::<String>()),
            "error": result.as_ref().err().map(ToString::to_string),
        }),
    );
    result
}

/// Stall-planner vision chain: the benchmark's strongest vision PLANNERS first
/// (`gemini-flash-lite` = 2.5-flash-lite — fast, no harmful picks; `gemini-flash`
/// = 2.5-flash — top score), then the abundant 3.1-flash-lite default takes over
/// via the standard fallback chain (those 2.5 ids are 20/day; stalls are rare).
const PLANNER_VISION_PREFER: &[&str] = &["gemini-flash-lite", "gemini-flash"];

impl Brain {
    /// Some goals are completed by speaking/returning information, not by changing
    /// the screen. A screen-only verifier will reject those and make the model read
    /// the same answer again. Accept them when a recent read/info tool succeeded.
    fn informational_done_verdict(&self, task: &str) -> Option<String> {
        let lower = task.to_lowercase();
        let wants_spoken_answer = [
            "read",
            "read out",
            "read aloud",
            "say",
            "tell me",
            "summarize",
            "summary",
            "explain",
            "answer",
            "what",
            "who",
            "clipboard",
            "clip",
            "lore",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
        let visible_destination = [
            "paste",
            "put into",
            "copy into",
            "word",
            "document",
            "file",
            "send",
            "submit",
            "click",
            "turn on",
            "change",
            "create",
            "install",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
        if !wants_spoken_answer || visible_destination {
            return None;
        }
        let info_tool = self.trail.iter().rev().find(|entry| {
            [
                "read_clipboard=ok",
                "browser_read_page=ok",
                "browser_extract_page=ok",
                "artifact_info=ok",
                "search_memory=ok",
                "open_memory=ok",
                "look=ok",
            ]
            .iter()
            .any(|prefix| entry.starts_with(prefix))
        })?;
        Some(format!(
            "YES - accepted as an informational/spoken-output task; recent evidence: {info_tool}. Screen state is not the completion signal for this task."
        ))
    }

    /// Independent high-res vision check of a `done` claim. Returns (accepted,
    /// verdict text). Checker errors are unknown and fail closed.
    pub fn verify_done(&self, task: &str, cancel: &AtomicBool) -> (bool, String) {
        if let Some(verdict) = self.informational_done_verdict(task) {
            return (true, verdict);
        }
        let full = window_view(self.target.as_deref(), self.whole_screen);
        let q = format!(
            "A computer agent claims this task is COMPLETE: \"{task}\". \
If the task is INFORMATIONAL - to read, summarize, explain, identify, find, compare or report what something is \
(the deliverable is the agent's spoken answer; there is NO visible 'done' state on screen) - answer YES as long as \
the relevant content is visible or has clearly been read. \
If the task is an ACTION with a visible end-state (a setting changed, a form submitted, an item placed, a level won) \
- answer YES only if that end-state is actually visible right now, otherwise NO. \
Start your answer with YES or NO, then quote the exact on-screen evidence (or state what is shown instead)."
        );
        classify_done_read(read_view(full, &q, &format!("task: {task}"), cancel))
    }

    /// Stall safety-net — the "merge" planner: ONE grounded vision call that SEES
    /// the screen and proposes the single best NEXT action when the agent is
    /// looping (perception + plan in one shot). Prefers the fast/accurate 2.5
    /// vision models. None on vision failure (the static stuck_warning note still
    /// stands). Cancellable, so a barge-in returns immediately.
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
        let advice = read_view_pref(
            view,
            &q,
            &format!("task: {task}"),
            cancel,
            PLANNER_VISION_PREFER,
        )
        .ok()?
        .trim()
        .to_string();
        (!advice.is_empty()).then_some(advice)
    }

    pub fn final_review(&self, note: &str) {
        final_review(&self.dir, self.target.as_deref(), note);
    }
}

/// A missing independent reading is an unknown outcome, never proof that an
/// action task succeeded. This also keeps cancellation from becoming fail-open.
fn classify_done_read(reading: Result<String>) -> (bool, String) {
    match reading {
        Ok(answer) => (
            answer.trim_start().to_lowercase().starts_with("yes"),
            answer,
        ),
        Err(error) => (
            false,
            format!("UNKNOWN - independent vision check unavailable: {error}"),
        ),
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
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
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
    let (fresh_jpeg, _) = session::encode_view(&fresh, view, VISION_SHORT, None, None)?;
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
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
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
    let Ok((fine_jpeg, shown)) = session::encode_view(cap, zoom, VISION_SHORT, None, None) else {
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

/// Locate `description` in the controlled browser's viewport via a CDP screenshot,
/// returning the point in CSS px (the space CDP input uses) plus what was seen.
pub(super) fn locate_css(
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<(f64, f64, Option<String>)> {
    let (jpeg, cw, ch) = super::super::browser::shot()?;
    let (d, c) = (description.to_string(), ctx.to_string());
    let loc = run_cancellable(cancel, move || {
        super::super::vision_reader::locate_point(&jpeg, &d, &c)
    })?;
    let (fresh_jpeg, fresh_w, fresh_h) = super::super::browser::shot()?;
    if (fresh_w - cw).abs() > f64::EPSILON || (fresh_h - ch).abs() > f64::EPSILON {
        anyhow::bail!("browser viewport changed while locating the target");
    }
    let loc = verify_located(&fresh_jpeg, loc, description, ctx, cancel)?;
    Ok((loc.x / 1000.0 * cw, loc.y / 1000.0 * ch, loc.note))
}

/// click_target via the trusted browser pipeline.
pub(super) fn browser_click(desc: &str, right: bool, ctx: &str, cancel: &AtomicBool) -> Value {
    match locate_css(desc, ctx, cancel) {
        Ok((x, y, note)) => {
            eprintln!("[cc] CLICK_TARGET(browser) '{desc}' -> css({x:.0},{y:.0}) saw={note:?}");
            match super::super::browser::click(x, y, right) {
                Ok(()) => {
                    json!({"ok": true, "via": "browser", "css_px": [x.round(), y.round()], "saw_at_target": note})
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            }
        }
        Err(e) => json!({"ok": false, "error": format!("could not locate '{desc}': {e}")}),
    }
}

/// drag_target via the trusted browser pipeline (vision-locate BOTH endpoints).
pub(super) fn browser_drag(from: &str, to: &str, ctx: &str, cancel: &AtomicBool) -> Value {
    let f = match locate_css(from, ctx, cancel) {
        Ok(v) => v,
        Err(e) => {
            return json!({"ok": false, "error": format!("could not locate from '{from}': {e}")});
        }
    };
    let t = match locate_css(to, ctx, cancel) {
        Ok(v) => v,
        Err(e) => return json!({"ok": false, "error": format!("could not locate to '{to}': {e}")}),
    };
    eprintln!(
        "[cc] DRAG_TARGET(browser) '{from}'->'{to}' : css({:.0},{:.0})->({:.0},{:.0})",
        f.0, f.1, t.0, t.1
    );
    match super::super::browser::drag(f.0, f.1, t.0, t.1) {
        Ok(()) => json!({"ok": true, "via": "browser", "from": f.2, "to": t.2,
            "from_css": [f.0.round(), f.1.round()], "to_css": [t.0.round(), t.1.round()]}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

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
mod tests {
    use super::classify_done_read;

    #[test]
    fn completion_check_errors_fail_closed() {
        let (accepted, verdict) = classify_done_read(Err(anyhow::anyhow!("cancelled")));
        assert!(!accepted);
        assert!(verdict.starts_with("UNKNOWN"));
    }

    #[test]
    fn completion_check_requires_explicit_yes() {
        assert!(classify_done_read(Ok("YES - visible".into())).0);
        assert!(!classify_done_read(Ok("NO - not visible".into())).0);
    }
}
