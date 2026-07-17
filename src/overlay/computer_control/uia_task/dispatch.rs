//! Brain::dispatch - the per-tool-call execution (the big match arm), split from
//! brain.rs for the file-size limit. `use super::*` pulls in the shared imports,
//! helper fns, and the `Brain` type just like brain.rs does.

use super::*;

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
                    match (
                        locate_in_view(self.view, from, ctx, cancel),
                        locate_in_view(self.view, to, ctx, cancel),
                    ) {
                        (Ok(f), Ok(t)) => {
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
                        (Err(e), _) => {
                            json!({"ok": false, "error": format!("could not locate from '{from}': {e}")})
                        }
                        (_, Err(e)) => {
                            json!({"ok": false, "error": format!("could not locate to '{to}': {e}")})
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
            "type_text" | "key_combination" | "click_here" => {
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    match guarded_direct_input_args(
                        name,
                        args.clone(),
                        self.target.as_deref(),
                        self.source_frame.as_ref(),
                        self.keyboard_target_gate.refocus_required(),
                    ) {
                        Ok(guarded) => executor::execute_ex(name, &guarded, &self.profile, cancel),
                        Err(error) => dispatch_telemetry::pre_dispatch_failure(error),
                    }
                }
            }
            "open_url" | "launch_app" => {
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    executor::execute_ex(name, args, &self.profile, cancel)
                }
            }
            "run_command" => {
                self.dispatch_exact_process(args, cancel, action, authorize_repair_process)
            }
            "edit_text_file" => self.dispatch_text_edit(args, cancel, action),
            "edit_text_file_structure" => self.dispatch_structural_edit(args, cancel, action),
            "save_artifact" => self.dispatch_artifact_save(args, cancel, action),
            "system_query" => super::super::system_query::query(args),
            "list_files" => super::super::system_query::list_files(args),
            "read_text_file" => executor::execute_ex(name, args, &self.profile, cancel),
            "scroll" => {
                // Real mouse-wheel scroll. Resolve where to scroll: a given grid
                // cell, else the centre of the current view (the wheel acts on the
                // window under that point).
                let (mx, my) = args
                    .get("cell")
                    .and_then(Value::as_u64)
                    .and_then(|c| self.grid.center_norm(c as u32))
                    .unwrap_or((500.0, 500.0));
                let (sx, sy) = self.view.to_screen_px(mx, my);
                // executor scroll/drag take 0..1000 normalized, not screen px.
                let (nx, ny) = executor::screen_to_norm(sx, sy);
                let a = json!({
                    "x": nx, "y": ny,
                    "direction": args.get("direction").and_then(Value::as_str).unwrap_or("down"),
                    "magnitude": args.get("amount").and_then(Value::as_f64).unwrap_or(5.0),
                });
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    match guarded_input_args(a, self.target.as_deref(), self.source_frame.as_ref())
                    {
                        Ok(guarded) => {
                            executor::execute_ex("scroll", &guarded, &self.profile, cancel)
                        }
                        Err(error) => dispatch_telemetry::pre_dispatch_failure(error),
                    }
                }
            }
            "drag" => {
                // Press at from_cell, glide, release at to_cell — for sliders,
                // reordering, drawing, or click-drag selection. Zoom first for
                // finer cells when precision matters.
                let from = args
                    .get("from_cell")
                    .and_then(Value::as_u64)
                    .and_then(|c| self.grid.center_norm(c as u32));
                let to = args
                    .get("to_cell")
                    .and_then(Value::as_u64)
                    .and_then(|c| self.grid.center_norm(c as u32));
                match (from, to) {
                    (Some((fx, fy)), Some((tx, ty))) => {
                        // executor drag takes 0..1000 normalized, not screen px.
                        let (fpx, fpy) = self.view.to_screen_px(fx, fy);
                        let (tpx, tpy) = self.view.to_screen_px(tx, ty);
                        let (sx, sy) = executor::screen_to_norm(fpx, fpy);
                        let (dx, dy) = executor::screen_to_norm(tpx, tpy);
                        let a = json!({"x": sx, "y": sy, "dest_x": dx, "dest_y": dy});
                        if self.dry {
                            json!({"ok": true, "note": "dry"})
                        } else {
                            match guarded_input_args(
                                a,
                                self.target.as_deref(),
                                self.source_frame.as_ref(),
                            ) {
                                Ok(guarded) => {
                                    executor::execute_ex("drag", &guarded, &self.profile, cancel)
                                }
                                Err(error) => dispatch_telemetry::pre_dispatch_failure(error),
                            }
                        }
                    }
                    _ => json!({"ok": false, "error": "drag needs from_cell and to_cell"}),
                }
            }
            "focus_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                self.keyboard_target_gate.begin_focus_attempt();
                match super::super::uia::raise_window_with_target(title) {
                    Err(error) => window_error(error),
                    Ok((raised, target)) => {
                        self.keyboard_target_gate.record_focus_result(raised);
                        std::thread::sleep(Duration::from_millis(200));
                        if raised {
                            // Re-frame on the newly focused window and discard stale anchors.
                            self.whole_screen = false;
                            self.zoomed = false;
                            self.clear_anchors("focused_different_window");
                        }
                        let now = super::super::uia::pointer_context().0;
                        json!({
                            "ok": raised,
                            "target": target,
                            "foreground_now": now,
                            "effect_verified": raised,
                            "effect_may_have_occurred": true,
                            "executed": raised.then_some(true),
                            "note": if raised { "switched" } else { "BLOCKED: the resolved window did not become foreground. Do not repeat the same focus attempt blindly; use a non-foreground provider when one exposes the needed state, otherwise report the blocker." }
                        })
                    }
                }
            }
            "list_windows" => {
                json!({"ok": true, "windows": super::super::uia::list_windows()})
            }
            "read_clipboard" => {
                json!({"ok": true, "text": super::super::clipboard::get_text()})
            }
            "minimize_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                match super::super::uia::minimize_window(title) {
                    Err(error) => window_error(error),
                    Ok(ok) => {
                        std::thread::sleep(Duration::from_millis(200));
                        json!({"ok": ok, "foreground_now": super::super::uia::pointer_context().0})
                    }
                }
            }
            "resize_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let w = args.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
                let h = args.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;
                match super::super::uia::resize_window(title, w, h) {
                    Ok(ok) => json!({"ok": ok}),
                    Err(error) => window_error(error),
                }
            }
            "move_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let x = args.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
                let y = args.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
                match super::super::uia::move_window(title, x, y) {
                    Ok(ok) => json!({"ok": ok}),
                    Err(error) => window_error(error),
                }
            }
            "search_memory" => {
                let query = args.get("query").and_then(Value::as_str).unwrap_or("");
                let hits = super::super::memory::search(query, 5);
                if hits.is_empty() {
                    json!({"ok": true, "results": [], "note": "no matching past conversation"})
                } else {
                    let results: Vec<Value> = hits
                        .iter()
                        .map(|h| {
                            json!({"id": h.id.to_string(), "when": h.timestamp, "title": h.title, "snippet": h.snippet})
                        })
                        .collect();
                    json!({"ok": true, "results": results, "instruction": "Results are ranked by relevance + recency; each has a 'when' timestamp. For 'the last/most recent/previous conversation', pick the one with the newest 'when'. Then open_memory(id) to read it in full."})
                }
            }
            "open_memory" => {
                let id = args.get("id").and_then(Value::as_str).unwrap_or("");
                match id.parse::<i64>().ok().and_then(super::super::memory::open) {
                    Some(transcript) => json!({"ok": true, "transcript": transcript}),
                    None => json!({"ok": false, "error": "no saved conversation with that id"}),
                }
            }
            "list_app_integrations" => super::super::mcp::list_tool(),
            "setup_app_integration" => super::super::mcp::setup_tool(
                args.get("id").and_then(Value::as_str).unwrap_or(""),
                args.get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            "app_integration_status" => {
                super::super::mcp::status_tool(args.get("id").and_then(Value::as_str).unwrap_or(""))
            }
            "read_app_integration_docs" => {
                super::super::mcp::docs_tool(args.get("id").and_then(Value::as_str).unwrap_or(""))
            }
            "remove_app_integration" => {
                super::super::mcp::remove_tool(args.get("id").and_then(Value::as_str).unwrap_or(""))
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

fn window_error(error: super::super::uia::WindowError) -> Value {
    json!({
        "ok": false,
        "code": error.code(),
        "error": error.to_string(),
    })
}
