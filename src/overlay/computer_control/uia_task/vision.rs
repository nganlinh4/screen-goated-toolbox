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
    let (jpeg, shown) = session::encode_view(&cap, view, VISION_SHORT, None, None, None)
        .inspect_err(|error| {
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

/// Bounded planner-grade preference before the normal vision fallback chain.
/// This affects only auxiliary recovery advice; execution still requires fresh
/// grounding and the ordinary structural action guards.
const PLANNER_VISION_PREFER: &[&str] = &["gemini-flash-lite", "gemini-flash"];

impl Brain {
    /// Independent high-res vision check of a `done` claim. Returns (accepted,
    /// typed verdict). Checker errors are unavailable and fail closed.
    pub fn verify_done(&self, task: &str, cancel: &AtomicBool) -> DoneCheck {
        let full = window_view(self.target.as_deref(), self.whole_screen);
        let q = done_verifier_question(task);
        classify_done_read(read_view(
            full,
            &q,
            &self.done_verifier_context(task),
            cancel,
        ))
    }

    pub(super) fn done_verifier_context(&self, task: &str) -> String {
        format!(
            "task: {task}; bounded provenance-labelled evidence this turn:\n{}",
            self.completion_evidence.context()
        )
    }

    /// Stall safety-net — the "merge" planner: ONE grounded vision call that SEES
    /// the screen and proposes the single best NEXT action when the agent is
    /// looping (perception + plan in one shot). Prefers the fast/accurate 2.5
    /// vision models. None on vision failure; the compact typed postcondition
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

    pub fn final_review(&self, task: &str, note: &str) {
        final_review(&self.dir, self.target.as_deref(), task, note);
    }
}

fn done_verifier_question(task: &str) -> String {
    format!(
        "Independently evaluate whether the current visible state and supplied receipts prove this requested outcome: {task:?}. \
Do not trust the acting agent's claim or infer an effect from intent. Evidence provenance is strict: job_source proves only the exact surface where the job began; grounded_surface proves only its exact title, URL, and identity; capability_result proves only the fields that capability directly returned (an ok status proves execution, not a semantic premise); model_inference is advisory and cannot by itself establish a factual, ordinal, comparative, or relational premise; model_authored_computation proves only that supplied code ran, never that its output reflects provider state; model_mediated_effect proves execution metadata but its inferred target label is advisory. A click or navigation receipt proves the interaction, not that its target satisfied a requested selection rule. Build a complete evidence chain, preserve any source-identity constraint in the task, and return complete=false when a required premise is missing or contradicted. A communicative outcome may be proven by a direct read receipt even when there is no terminal visual state; a state-change outcome requires the requested postcondition. \
Return one JSON object only: {{\"complete\":boolean,\"evidence\":\"specific supporting state or receipt\",\"contradiction\":\"specific missing or conflicting state, or empty\"}}."
    )
}

/// A missing independent reading is an unknown outcome, never proof that an
/// action task succeeded. This also keeps cancellation from becoming fail-open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::overlay::computer_control) struct DoneCheck {
    pub(in crate::overlay::computer_control) complete: bool,
    pub(in crate::overlay::computer_control) unavailable: bool,
    pub(in crate::overlay::computer_control) verdict: String,
}

fn classify_done_read(reading: Result<String>) -> DoneCheck {
    match reading {
        Ok(answer) => match parse_done_verdict(&answer) {
            Some((complete, evidence, contradiction)) => {
                let verdict = serde_json::json!({
                    "complete": complete,
                    "evidence": evidence,
                    "contradiction": contradiction,
                })
                .to_string();
                DoneCheck {
                    complete,
                    unavailable: false,
                    verdict,
                }
            }
            None => DoneCheck {
                complete: false,
                unavailable: true,
                verdict: "independent vision check returned an invalid verdict schema".to_string(),
            },
        },
        Err(error) => DoneCheck {
            complete: false,
            unavailable: true,
            verdict: format!("independent vision check unavailable: {error}"),
        },
    }
}

fn parse_done_verdict(answer: &str) -> Option<(bool, String, String)> {
    let start = answer.find('{')?;
    let end = answer.rfind('}')?;
    let value: Value = serde_json::from_str(&answer[start..=end]).ok()?;
    let complete = value.get("complete")?.as_bool()?;
    let evidence = value.get("evidence")?.as_str()?.trim().to_string();
    let contradiction = value
        .get("contradiction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if (complete && (evidence.is_empty() || !contradiction.is_empty()))
        || (!complete && contradiction.is_empty())
    {
        return None;
    }
    Some((complete, evidence, contradiction))
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
mod tests {
    use super::{classify_done_read, done_verifier_question};

    #[test]
    fn completion_check_errors_fail_closed() {
        let result = classify_done_read(Err(anyhow::anyhow!("cancelled")));
        assert!(!result.complete);
        assert!(result.unavailable);
    }

    #[test]
    fn completion_check_uses_typed_verdict_not_language_phrases() {
        assert!(
            classify_done_read(Ok(
                r#"{"complete":true,"evidence":"opaque-evidence-17","contradiction":""}"#.into()
            ))
            .complete
        );
        let prose = classify_done_read(Ok("YES - visible".into()));
        assert!(!prose.complete);
        assert!(prose.unavailable);
        assert!(!classify_done_read(Ok(
            r#"{"complete":false,"evidence":"opaque-observation-2","contradiction":"opaque-contradiction-9"}"#
                .into()
        ))
        .complete);
        assert!(
            !classify_done_read(Ok(
                r#"{"complete":true,"evidence":"state","contradiction":"missing"}"#.into()
            ))
            .complete
        );
    }

    #[test]
    fn incomplete_typed_verdict_needs_only_a_contradiction() {
        let result = classify_done_read(Ok(
            r#"{"complete":false,"evidence":"","contradiction":"opaque-contradiction-11"}"#.into(),
        ));
        assert!(!result.complete);
        assert!(!result.unavailable);
        assert!(result.verdict.contains("opaque-contradiction-11"));

        let invalid = classify_done_read(Ok(
            r#"{"complete":false,"evidence":"opaque-observation-4","contradiction":""}"#.into(),
        ));
        assert!(!invalid.complete);
        assert!(invalid.unavailable);
    }

    #[test]
    fn verifier_contract_does_not_promote_inference_or_effect_receipts_to_proof() {
        let question = done_verifier_question("opaque task");
        assert!(question.contains("model_inference is advisory"));
        assert!(question.contains("model_authored_computation proves only"));
        assert!(question.contains("ordinal, comparative, or relational premise"));
        assert!(question.contains("A click or navigation receipt proves the interaction"));
        assert!(question.contains("source-identity constraint"));
        assert!(question.contains("complete=false"));
    }
}
