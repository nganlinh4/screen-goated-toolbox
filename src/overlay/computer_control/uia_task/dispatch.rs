//! Brain::dispatch - the per-tool-call execution (the big match arm), split from
//! brain.rs for the file-size limit. `use super::*` pulls in the shared imports,
//! helper fns, and the `Brain` type just like brain.rs does.

use super::*;

mod context_tools;
mod native_tools;

impl Brain {
    /// Execute one tool call (NOT `done`). Returns the action result JSON; polls
    /// `cancel` (set on barge-in) between micro-steps via the humanized executor.
    pub fn dispatch(
        &mut self,
        name: &str,
        args: &Value,
        ctx: &str,
        cancel: &Arc<AtomicBool>,
        trace: Option<super::super::telemetry::ActionTrace>,
        authorize_repair_process: bool,
    ) -> Value {
        self.step += 1;
        let step = self.step;
        let action = trace.unwrap_or_else(|| super::super::telemetry::claim_action(name));
        self.active_action = Some(action);
        let t0 = Instant::now();
        if action_invalidates_anchors(name) {
            self.clear_anchors(&format!("before_{name}"));
        }
        // Strengthen the (stateless) aux models' context: hand them what the agent has already DONE
        // this task (last few actions), not just the one-line task+intent — so "the other one" / "the
        // next button" disambiguate and the stall planner sees the trajectory. ~Free: a few tokens.
        let enriched_ctx;
        let ctx: &str = if self.trail.is_empty() {
            ctx
        } else {
            let recent = &self.trail[self.trail.len().saturating_sub(6)..];
            enriched_ctx = format!("{ctx}; already did: {}", recent.join("  ->  "));
            &enriched_ctx
        };
        if let Some(blocked) = self.setup_guard.before_action(name) {
            return self.finish_dispatch(action, name, args, blocked, t0);
        }
        if let Some(blocked) = self.exact_edit_guard.before_action(name, args) {
            return self.finish_dispatch(action, name, args, blocked, t0);
        }
        if let Some(result) = self.dispatch_browser_tool(name, args, cancel) {
            return self.finish_dispatch(action, name, args, result, t0);
        }
        if let Some(result) = self.dispatch_context_tool(name, args) {
            return self.finish_dispatch(action, name, args, result, t0);
        }
        if let Some(result) =
            self.dispatch_native_tool(name, args, cancel, action, authorize_repair_process)
        {
            return self.finish_dispatch(action, name, args, result, t0);
        }
        let result = match name {
            // Deterministic controller (Stage 1): the model reads the indexed world
            // and acts by @id; the controller resolves/executes/verifies/gates.
            "observe" => self.controller.observe(),
            "act" => {
                let act_ctx = super::super::controller::ActCtx {
                    profile: &self.profile,
                    cancel,
                    dry: self.dry,
                };
                self.controller.act(
                    args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32,
                    args.get("verb").and_then(Value::as_str).unwrap_or(""),
                    args.get("value").and_then(Value::as_str),
                    args.get("confirm")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    &act_ctx,
                )
            }
            "do_steps" => {
                let act_ctx = super::super::controller::ActCtx {
                    profile: &self.profile,
                    cancel,
                    dry: self.dry,
                };
                let steps = args
                    .get("steps")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                self.controller.do_steps(&steps, &act_ctx)
            }
            "click_at" => {
                let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                if let Some(blocked) = super::dispatch_guard::block_grid_click(
                    self.view,
                    &self.grid,
                    cell,
                    self.target.as_deref(),
                ) {
                    return self.finish_dispatch(action, name, args, blocked, t0);
                }
                match self.grid.center_norm(cell) {
                    Some((mx, my)) => {
                        let (sx, sy) = self.view.to_screen_px(mx, my);
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(
                            &self.dir,
                            action,
                            json!({"step": step, "kind": "click_at", "cell": cell,
                            "view_norm": [mx.round(), my.round()], "screen_px": [sx, sy],
                            "view_rect": [self.view.x, self.view.y, self.view.w, self.view.h]}),
                        );
                        let input = click_screen(
                            sx,
                            sy,
                            "left",
                            InputContext {
                                dry: self.dry,
                                profile: &self.profile,
                                cancel,
                                target: self.target.as_deref(),
                                source: self.source_frame.as_ref(),
                            },
                        );
                        pointer_result(
                            input,
                            self.view,
                            (mx, my),
                            (sx, sy),
                            json!({"kind": "click_at", "cell": cell}),
                        )
                    }
                    None => {
                        json!({"ok": false, "error": format!("cell {cell} out of range 1..={}", self.grid.cell_count())})
                    }
                }
            }
            "zoom" => {
                let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                match zoom_to_cell(self.view, &self.grid, cell) {
                    Some(v) => {
                        self.view = v;
                        self.zoomed = true;
                        self.clear_anchors("zoom_changed_view");
                        json!({"ok": true, "zoomed_cell": cell})
                    }
                    None => {
                        json!({"ok": false, "error": format!("cell {cell} out of range 1..={}", self.grid.cell_count())})
                    }
                }
            }
            "reset_view" => {
                self.zoomed = false;
                self.whole_screen = false;
                self.clear_anchors("reset_view");
                json!({"ok": true, "view": "the active window"})
            }
            "see_whole_screen" => {
                // Switch the base view to the WHOLE desktop for awareness / to find
                // or reach another window. reset_view (or focus_window) goes back to
                // the precise active-window view.
                self.whole_screen = true;
                self.zoomed = false;
                self.clear_anchors("whole_screen_view");
                json!({"ok": true, "view": "the whole screen"})
            }
            "look" => {
                let q = args
                    .get("question")
                    .and_then(Value::as_str)
                    .unwrap_or("Describe exactly what is on screen.");
                match read_view(self.view, q, ctx, cancel) {
                    Ok(answer) => {
                        eprintln!("[cc] step {step:02} LOOK: {answer}");
                        json!({"ok": true, "reading": answer})
                    }
                    Err(e) => json!({"ok": false, "error": format!("vision read failed: {e}")}),
                }
            }
            "click_target" => {
                let desc = args
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let button = match args.get("button").and_then(Value::as_str) {
                    Some("right") => "right",
                    _ => "left",
                };
                // In a Chromium browser, drive the click through the page's OWN
                // trusted input (CDP) so canvas/WebGL games + cross-origin iframes
                // that ignore synthetic OS clicks respond — and with crisper coords.
                let browser_target = match browser_vision_target(
                    self.controlled_tab_id,
                    self.source_frame.as_ref(),
                ) {
                    Ok(target) => target,
                    Err(error) => {
                        return self.finish_dispatch(
                            action,
                            name,
                            args,
                            json!({"ok": false, "code": "ERR_STALE_FRAME_SURFACE", "error": error.to_string()}),
                            t0,
                        );
                    }
                };
                if !self.dry
                    && let Some(browser_target) = browser_target
                {
                    browser_click(browser_target, desc, button == "right", ctx, cancel)
                } else {
                    match locate_in_view(self.view, desc, ctx, cancel) {
                        Ok(loc) => {
                            if let Err(error) = revalidate_visual_locations(self.view, &[&loc]) {
                                return self.finish_dispatch(
                                    action,
                                    name,
                                    args,
                                    json!({
                                        "ok": false,
                                        "code": "ERR_STALE_VISUAL_TARGET",
                                        "effect_may_have_occurred": false,
                                        "error": error.to_string(),
                                    }),
                                    t0,
                                );
                            }
                            let (sx, sy) = self.view.to_screen_px(loc.x, loc.y);
                            self.last_click = Some((sx, sy));
                            self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                            append_click(
                                &self.dir,
                                action,
                                json!({"step": step, "kind": "click_target", "desc": desc,
                                "button": button, "view_norm": [loc.x.round(), loc.y.round()],
                                "screen_px": [sx, sy], "saw": loc.note,
                                "view_rect": [self.view.x, self.view.y, self.view.w, self.view.h]}),
                            );
                            eprintln!(
                                "[cc] step {step:02} CLICK_TARGET[{button}] '{desc}' -> screen({sx},{sy}) saw={:?}",
                                loc.note
                            );
                            let input = click_screen(
                                sx,
                                sy,
                                button,
                                InputContext {
                                    dry: self.dry,
                                    profile: &self.profile,
                                    cancel,
                                    target: self.target.as_deref(),
                                    source: self.source_frame.as_ref(),
                                },
                            );
                            pointer_result(
                                input,
                                self.view,
                                (loc.x, loc.y),
                                (sx, sy),
                                json!({"kind": "click_target", "saw_at_target": loc.note}),
                            )
                        }
                        Err(e) => {
                            json!({"ok": false, "error": format!("could not locate '{desc}': {e}")})
                        }
                    }
                }
            }
            "drag_target" => {
                // Precise drag: vision-locate BOTH endpoints and drag between them -
                // for canvas drag-and-drop (place a card on a slot, move a slider).
                let from = args.get("from").and_then(Value::as_str).unwrap_or("");
                let to = args.get("to").and_then(Value::as_str).unwrap_or("");
                // In a Chromium browser, drag through the page's trusted input (CDP):
                // canvas/WebGL + HTML5 drag-and-drop ignore synthetic OS drags.
                let browser_target = match browser_vision_target(
                    self.controlled_tab_id,
                    self.source_frame.as_ref(),
                ) {
                    Ok(target) => target,
                    Err(error) => {
                        return self.finish_dispatch(
                            action,
                            name,
                            args,
                            json!({"ok": false, "code": "ERR_STALE_FRAME_SURFACE", "error": error.to_string()}),
                            t0,
                        );
                    }
                };
                if !self.dry
                    && let Some(browser_target) = browser_target
                {
                    browser_drag(browser_target, from, to, ctx, cancel)
                } else {
                    match drag_in_view(self.view, from, to, ctx, cancel) {
                        Ok((f, t)) => {
                            if let Err(error) = revalidate_visual_locations(self.view, &[&f, &t]) {
                                return self.finish_dispatch(
                                    action,
                                    name,
                                    args,
                                    json!({
                                        "ok": false,
                                        "code": "ERR_STALE_VISUAL_TARGET",
                                        "effect_may_have_occurred": false,
                                        "error": error.to_string(),
                                    }),
                                    t0,
                                );
                            }
                            let (fsx, fsy) = self.view.to_screen_px(f.x, f.y);
                            let (tsx, tsy) = self.view.to_screen_px(t.x, t.y);
                            self.last_click = Some((tsx, tsy));
                            self.click_before = session::capture_region_fp(tsx, tsy, VC_HALF);
                            append_click(
                                &self.dir,
                                action,
                                json!({"step": step, "kind": "drag_target", "from": from, "to": to,
                                "from_px": [fsx, fsy], "to_px": [tsx, tsy], "saw_from": f.note, "saw_to": t.note}),
                            );
                            eprintln!(
                                "[cc] step {step:02} DRAG_TARGET '{from}' -> '{to}' : ({fsx},{fsy})->({tsx},{tsy})"
                            );
                            let r = drag_screen(
                                (fsx, fsy),
                                (tsx, tsy),
                                InputContext {
                                    dry: self.dry,
                                    profile: &self.profile,
                                    cancel,
                                    target: self.target.as_deref(),
                                    source: self.source_frame.as_ref(),
                                },
                            );
                            json!({"ok": true, "from": f.note, "to": t.note, "drag": r})
                        }
                        Err(e) => {
                            json!({"ok": false, "error": format!("could not locate drag endpoints: {e}")})
                        }
                    }
                }
            }
            "point_at" => {
                // Same vision-locate as click_target, but MOVE the cursor onto the
                // target and stop - no click. For "point at / show me X" or to hover
                // and reveal a tooltip / hover-menu (dwell_seconds lets it surface).
                let desc = args
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let dwell = args
                    .get("dwell_seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .clamp(0.0, 10.0);
                match locate_in_view(self.view, desc, ctx, cancel) {
                    Ok(loc) => {
                        if let Err(error) = revalidate_visual_locations(self.view, &[&loc]) {
                            return self.finish_dispatch(
                                action,
                                name,
                                args,
                                json!({
                                    "ok": false,
                                    "code": "ERR_STALE_VISUAL_TARGET",
                                    "effect_may_have_occurred": false,
                                    "error": error.to_string(),
                                }),
                                t0,
                            );
                        }
                        let (sx, sy) = self.view.to_screen_px(loc.x, loc.y);
                        self.last_click = Some((sx, sy)); // mark where we pointed on the next frame
                        append_click(
                            &self.dir,
                            action,
                            json!({"step": step, "kind": "point_at", "desc": desc,
                            "view_norm": [loc.x.round(), loc.y.round()], "screen_px": [sx, sy],
                            "saw": loc.note, "view_rect": [self.view.x, self.view.y, self.view.w, self.view.h]}),
                        );
                        eprintln!(
                            "[cc] step {step:02} POINT_AT '{desc}' -> screen({sx},{sy}) saw={:?}",
                            loc.note
                        );
                        let input = point_screen(
                            sx,
                            sy,
                            (dwell * 1000.0) as u64,
                            InputContext {
                                dry: self.dry,
                                profile: &self.profile,
                                cancel,
                                target: self.target.as_deref(),
                                source: self.source_frame.as_ref(),
                            },
                        );
                        pointer_result(
                            input,
                            self.view,
                            (loc.x, loc.y),
                            (sx, sy),
                            json!({"kind": "point_at", "saw_at_target": loc.note}),
                        )
                    }
                    Err(e) => {
                        json!({"ok": false, "error": format!("could not point at '{desc}': {e}")})
                    }
                }
            }
            "map_targets" | "click_mark" => {
                self.dispatch_anchor_action(name, args, ctx, cancel, action, step)
            }
            "wait" => {
                let secs = args
                    .get("seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(3.0)
                    .clamp(0.0, 30.0);
                let aborted = human_input::sleep_cancellable((secs * 1000.0) as u64, cancel);
                json!({"ok": !aborted, "waited_seconds": secs})
            }
            // Local artifact tools and installed MCP tools are dynamic-ish surfaces.
            _ => {
                super::super::artifacts::dispatch_tool(name, args, &self.profile, cancel, self.dry)
                    .or_else(|| super::super::mcp::try_dispatch(name, args))
                    .unwrap_or_else(|| json!({"ok": false, "error": "unknown action"}))
            }
        };
        self.finish_dispatch(action, name, args, result, t0)
    }
}
