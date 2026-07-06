//! Brain::dispatch - the per-tool-call execution (the big match arm), split from
//! brain.rs for the file-size limit. `use super::*` pulls in the shared imports,
//! helper fns, and the `Brain` type just like brain.rs does.

use super::*;

impl Brain {
    /// Execute one tool call (NOT `done`). Returns the action result JSON; polls
    /// `cancel` (set on barge-in) between micro-steps via the humanized executor.
    pub fn dispatch(&mut self, name: &str, args: &Value, ctx: &str, cancel: &AtomicBool) -> Value {
        self.step += 1;
        let step = self.step;
        let t0 = Instant::now();
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
            return blocked;
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
                    ctx,
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
                self.controller.do_steps(&steps, ctx, &act_ctx)
            }
            "click_at" => {
                let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                match self.grid.center_norm(cell) {
                    Some((mx, my)) => {
                        let (sx, sy) = self.view.to_screen_px(mx, my);
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(
                            &self.dir,
                            json!({"step": step, "kind": "click_at", "cell": cell,
                            "view_norm": [mx.round(), my.round()], "screen_px": [sx, sy],
                            "view": [self.view.x, self.view.y, self.view.w, self.view.h]}),
                        );
                        click_screen(sx, sy, self.dry, "left", &self.profile, cancel)
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
                        self.anchors.clear(); // view changed -> old anchors are stale
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
                self.anchors.clear();
                json!({"ok": true, "view": "the active window"})
            }
            "see_whole_screen" => {
                // Switch the base view to the WHOLE desktop for awareness / to find
                // or reach another window. reset_view (or focus_window) goes back to
                // the precise active-window view.
                self.whole_screen = true;
                self.zoomed = false;
                self.anchors.clear();
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
                if !self.dry && super::super::browser::input_active() {
                    browser_click(desc, button == "right", ctx, cancel)
                } else {
                    match locate_in_view(self.view, desc, ctx, cancel) {
                        Ok(loc) => {
                            let (sx, sy) = self.view.to_screen_px(loc.x, loc.y);
                            self.last_click = Some((sx, sy));
                            self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                            append_click(
                                &self.dir,
                                json!({"step": step, "kind": "click_target", "desc": desc,
                                "button": button, "view_norm": [loc.x.round(), loc.y.round()],
                                "screen_px": [sx, sy], "saw": loc.note,
                                "view": [self.view.x, self.view.y, self.view.w, self.view.h]}),
                            );
                            eprintln!(
                                "[cc] step {step:02} CLICK_TARGET[{button}] '{desc}' -> screen({sx},{sy}) saw={:?}",
                                loc.note
                            );
                            let r = click_screen(sx, sy, self.dry, button, &self.profile, cancel);
                            json!({"ok": true, "located_view_norm": [loc.x, loc.y], "saw_at_target": loc.note, "click": r})
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
                if !self.dry && super::super::browser::input_active() {
                    browser_drag(from, to, ctx, cancel)
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
                                json!({"step": step, "kind": "drag_target", "from": from, "to": to,
                                "from_px": [fsx, fsy], "to_px": [tsx, tsy], "saw_from": f.note, "saw_to": t.note}),
                            );
                            eprintln!(
                                "[cc] step {step:02} DRAG_TARGET '{from}' -> '{to}' : ({fsx},{fsy})->({tsx},{tsy})"
                            );
                            let r =
                                drag_screen(fsx, fsy, tsx, tsy, self.dry, &self.profile, cancel);
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
                            json!({"step": step, "kind": "point_at", "desc": desc,
                            "view_norm": [loc.x.round(), loc.y.round()], "screen_px": [sx, sy],
                            "saw": loc.note, "view": [self.view.x, self.view.y, self.view.w, self.view.h]}),
                        );
                        eprintln!(
                            "[cc] step {step:02} POINT_AT '{desc}' -> screen({sx},{sy}) saw={:?}",
                            loc.note
                        );
                        let r = point_screen(
                            sx,
                            sy,
                            (dwell * 1000.0) as u64,
                            self.dry,
                            &self.profile,
                            cancel,
                        );
                        json!({"ok": true, "pointed_view_norm": [loc.x, loc.y], "saw_at_target": loc.note, "move": r})
                    }
                    Err(e) => {
                        json!({"ok": false, "error": format!("could not point at '{desc}': {e}")})
                    }
                }
            }
            "map_targets" => {
                let desc = args
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match map_in_view(self.view, desc, ctx, cancel) {
                    Ok(pts) => {
                        self.anchors = pts
                            .iter()
                            .map(|p| {
                                let (sx, sy) = self.view.to_screen_px(p.x, p.y);
                                (sx, sy, p.note.clone())
                            })
                            .collect();
                        let list: Vec<Value> = self
                            .anchors
                            .iter()
                            .enumerate()
                            .map(|(i, (_, _, note))| json!({"mark": i + 1, "what": note}))
                            .collect();
                        eprintln!(
                            "[cc] step {step:02} MAP_TARGETS '{desc}' -> {} anchors",
                            self.anchors.len()
                        );
                        json!({"ok": true, "anchor_count": self.anchors.len(), "anchors": list,
                            "note": "Click any of these by its mark number with click_mark(mark). They stay valid until the layout changes - then re-run map_targets."})
                    }
                    Err(e) => json!({"ok": false, "error": format!("could not map '{desc}': {e}")}),
                }
            }
            "click_mark" => {
                let id = args.get("mark").and_then(Value::as_u64).unwrap_or(0) as usize;
                let button = match args.get("button").and_then(Value::as_str) {
                    Some("right") => "right",
                    _ => "left",
                };
                let anchor = self
                    .anchors
                    .get(id.wrapping_sub(1))
                    .map(|(sx, sy, n)| (*sx, *sy, n.clone()));
                match anchor {
                    Some((sx, sy, note)) => {
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(
                            &self.dir,
                            json!({"step": step, "kind": "click_mark", "mark": id,
                            "button": button, "screen_px": [sx, sy], "saw": note}),
                        );
                        eprintln!("[cc] step {step:02} CLICK_MARK {id} -> screen({sx},{sy})");
                        let r = click_screen(sx, sy, self.dry, button, &self.profile, cancel);
                        json!({"ok": true, "clicked_mark": id, "what": note, "click": r})
                    }
                    None => {
                        json!({"ok": false, "error": format!("no anchor #{id} (have {}); run map_targets first", self.anchors.len())})
                    }
                }
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
            "type_text" | "key_combination" | "open_url" | "launch_app" | "run_command"
            | "click_here" => {
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    executor::execute_ex(name, args, &self.profile, cancel)
                }
            }
            "system_query" => super::super::system_query::query(args),
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
                    executor::execute_ex("scroll", &a, &self.profile, cancel)
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
                            executor::execute_ex("drag", &a, &self.profile, cancel)
                        }
                    }
                    _ => json!({"ok": false, "error": "drag needs from_cell and to_cell"}),
                }
            }
            "focus_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let raised = super::super::uia::raise_window(title);
                std::thread::sleep(Duration::from_millis(200)); // let the switch settle
                if raised {
                    // Return to the precise active-window view (the prompt promises
                    // this): drop any whole-screen/zoom override and stale anchors so
                    // grounding re-frames on the newly-focused window.
                    self.whole_screen = false;
                    self.zoomed = false;
                    self.anchors.clear();
                }
                let now = super::super::uia::pointer_context().0;
                json!({
                    "ok": raised,
                    "foreground_now": now,
                    "note": if raised { "switched" } else { "BLOCKED: the foreground is holding the screen — an exclusive-fullscreen game won't let any window in front of it, ignores minimize, and swallows hotkeys. Do NOT retry this or minimize the game the user is playing. To read WEB content, use browser_read_page (it reads via the browser's debugger, no foreground needed); otherwise ask the user to alt-tab." }
                })
            }
            "list_windows" => {
                json!({"ok": true, "windows": super::super::uia::list_windows()})
            }
            "read_clipboard" => {
                json!({"ok": true, "text": super::super::clipboard::get_text()})
            }
            "minimize_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let ok = super::super::uia::minimize_window(title);
                std::thread::sleep(Duration::from_millis(200)); // let the minimize settle
                json!({"ok": ok, "foreground_now": super::super::uia::pointer_context().0})
            }
            "resize_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let w = args.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
                let h = args.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;
                json!({"ok": super::super::uia::resize_window(title, w, h)})
            }
            "move_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let x = args.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
                let y = args.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
                json!({"ok": super::super::uia::move_window(title, x, y)})
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
            "browser_setup" => super::super::browser::setup(),
            "browser_status" => super::super::browser::status(),
            "browser_reset" => super::super::browser::reset(),
            "browser_read_page" => super::super::browser::read_page(),
            "research_web" => super::super::research::research_web(args),
            "browser_extract_page" => super::super::browser::extract_page(),
            "browser_wait_for" => super::super::browser::wait_for(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(8000),
            ),
            "browser_eval" => super::super::browser::eval_js(
                args.get("code").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_navigate" => super::super::browser::navigate(
                args.get("url").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_open_tab" => super::super::browser::open_tab(
                args.get("url").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_upload" => super::super::browser::upload_file(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("path").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_tabs" => super::super::browser::get_tabs(),
            "browser_switch_tab" => super::super::browser::switch_tab(
                args.get("tab_id").and_then(Value::as_i64).unwrap_or(0),
            ),
            "browser_network" => super::super::browser::read_network(
                args.get("filter").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_console" => super::super::browser::read_console(),
            "decline_browser_control" => {
                super::super::browser::record_decline();
                json!({"ok": true, "noted": "won't ask again for a while"})
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
            "decline_app_integration" => super::super::mcp::decline_tool(
                args.get("id").and_then(Value::as_str).unwrap_or(""),
            ),
            // Local artifact tools and installed MCP tools are dynamic-ish surfaces.
            _ => {
                super::super::artifacts::dispatch_tool(name, args, &self.profile, cancel, self.dry)
                    .or_else(|| super::super::mcp::try_dispatch(name, args))
                    .unwrap_or_else(|| json!({"ok": false, "error": "unknown action"}))
            }
        };
        self.setup_guard.record_result(name, &result);
        // Per-action latency (excludes the settle wait) — the key refinement
        // signal for vision/click cost. Full result is truncated to avoid bloat;
        // look()/click_target log their rich detail on their own lines above.
        let ms = t0.elapsed().as_millis();
        let settle = if name == "open_url" || name == "launch_app" {
            1100
        } else {
            250
        };
        std::thread::sleep(Duration::from_millis(settle));
        // Rich, low-bloat per-tool log: observe/act surface their @id + verdict (so a
        // stale-id miss, a blocked gate, or a failed verify is VISIBLE at a glance);
        // every other tool gets a short truncated result.
        let short: String = match name {
            "observe" => format!(
                "{} elements",
                result.get("count").and_then(Value::as_u64).unwrap_or(0)
            ),
            "act" => {
                let id = args.get("id").and_then(Value::as_u64).unwrap_or(0);
                let verb = args.get("verb").and_then(Value::as_str).unwrap_or("act");
                let nm = result
                    .get("target")
                    .and_then(|t| t.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let outcome = result
                    .get("verify")
                    .and_then(Value::as_str)
                    .or_else(|| result.get("blocked").and_then(Value::as_str))
                    .or_else(|| result.get("error").and_then(Value::as_str))
                    .unwrap_or("ok");
                format!(
                    "{verb} @{id} {nm:?} -> {}",
                    outcome.chars().take(110).collect::<String>()
                )
            }
            "wait" => {
                let w = result
                    .get("waited_seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                format!(
                    "{w:.0}s (~{:.0}s total waiting — if nothing's changing, STOP)",
                    self.wait_accum + w
                )
            }
            _ => result.to_string().chars().take(120).collect(),
        };
        super::super::telemetry::human("cc", format!("step {step:02} {name} {ms}ms -> {short}"));
        super::super::telemetry::tool_result(
            name,
            step,
            ms,
            result.get("ok").and_then(Value::as_bool),
            json!({
                "result_preview": short,
                "blocked": result.get("blocked").cloned(),
                "error": result.get("error").cloned(),
                "code": result.get("code").cloned(),
            }),
        );
        // The controller's cached world is valid only right after observe/act; any
        // OTHER tool may have moved the screen, so invalidate it — the next act()
        // then re-syncs instead of resolving a STALE @id onto the wrong element.
        if !matches!(name, "observe" | "act") {
            self.controller.invalidate();
        }
        // Record the action trail (for situational context) + consecutive wait time.
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(true);
        self.trail
            .push(format!("{name}={}", if ok { "ok" } else { "fail" }));
        if self.trail.len() > 6 {
            self.trail.remove(0);
        }
        if name == "wait" {
            self.wait_accum += result
                .get("waited_seconds")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
        } else {
            self.wait_accum = 0.0;
        }
        result
    }
}
