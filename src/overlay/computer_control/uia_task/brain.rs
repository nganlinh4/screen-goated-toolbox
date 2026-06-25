//! The `Brain` impl — per-turn grounding, tool-call dispatch, and `done`
//! verification — split out of `uia_task.rs` for the file-size limit. `use
//! super::*` pulls in the shared imports, types, and render/vision helpers;
//! explicit `super::super::` paths reach the sibling CC modules.

use super::*;
impl Brain {
    pub fn new(target: Option<String>) -> Self {
        // Per-action frame + click traces (the key accuracy-refinement record).
        // Default to app-data so a released launch doesn't litter the cwd; a dev
        // run can override with CC_TRACE_DIR.
        let dir = std::env::var("CC_TRACE_DIR").unwrap_or_else(|_| {
            std::env::var("LOCALAPPDATA")
                .map(|p| format!("{p}/screen-goated-toolbox/cc-trace"))
                .unwrap_or_else(|_| "cc-trace".to_string())
        });
        std::fs::create_dir_all(&dir).ok();
        let view = window_view(target.as_deref(), false);
        Self {
            dir,
            grid: Grid::from_env(),
            profile: HumanProfile::from_env(),
            dry: std::env::var("CC_DRY").is_ok(),
            target,
            view,
            zoomed: false,
            whole_screen: false,
            last_click: None,
            step: 0,
            recent_actions: Vec::new(),
            prev_state_sig: None,
            click_before: None,
            trail: Vec::new(),
            wait_accum: 0.0,
            anchors: Vec::new(),
        }
    }

    /// Per-turn grounding context the model gets above the element list: where it
    /// is (window), where the cursor is + what's under it, what it just did, and
    /// how long it's been waiting. Cheap situational awareness.
    fn context_block(&self) -> String {
        let (title, cx, cy) = uia::pointer_context();
        let title: String = if title.is_empty() { "(unknown)".into() } else { title.chars().take(70).collect() };
        let trail = if self.trail.is_empty() { "(none yet)".to_string() } else { self.trail.join("  |  ") };
        let mut s = format!(
            "Active window: {title}\nCursor at ({cx},{cy})\nYour recent actions: {trail}"
        );
        if self.wait_accum > 0.0 {
            s.push_str(&format!(
                "\nWaited {:.0}s so far on this - if nothing has changed, stop waiting and act.",
                self.wait_accum
            ));
        }
        s
    }

    /// Turn-0 grounding: (frame_b64, state_text). No click marker yet.
    pub fn initial(&mut self) -> Result<(String, String)> {
        let elements = uia::enumerate(self.target.as_deref()).unwrap_or_default();
        let (b, v, _fp) = render_view(&self.dir, self.step, self.view, self.grid, None)?;
        self.view = v;
        self.prev_state_sig = Some(state_signature(&elements));
        let mut state = format_state(&elements, self.target.as_deref(), self.view, self.grid);
        if let Some(marks) = self.detect_marks(&elements) {
            state.push_str(&format!("\n{marks}"));
        }
        Ok((b, state))
    }

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
        let result = match name {
            "click_element" => {
                let elements = uia::enumerate(self.target.as_deref()).unwrap_or_default();
                let want = args.get("name").and_then(Value::as_str).unwrap_or("");
                let r = click_by_name(&elements, want, self.dry, &self.profile, cancel);
                if let Some(p) = r.get("screen_px").and_then(|v| v.as_array())
                    && p.len() == 2
                {
                    let (sx, sy) = (p[0].as_i64().unwrap_or(0) as i32, p[1].as_i64().unwrap_or(0) as i32);
                    self.last_click = Some((sx, sy));
                    append_click(&self.dir, json!({"step": step, "kind": "click_element", "name": want, "screen_px": [sx, sy]}));
                }
                r
            }
            "click_at" => {
                let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                match self.grid.center_norm(cell) {
                    Some((mx, my)) => {
                        let (sx, sy) = self.view.to_screen_px(mx, my);
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(&self.dir, json!({"step": step, "kind": "click_at", "cell": cell,
                            "view_norm": [mx.round(), my.round()], "screen_px": [sx, sy],
                            "view": [self.view.x, self.view.y, self.view.w, self.view.h]}));
                        click_screen(sx, sy, self.dry, "left", &self.profile, cancel)
                    }
                    None => json!({"ok": false, "error": format!("cell {cell} out of range 1..={}", self.grid.cell_count())}),
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
                    None => json!({"ok": false, "error": format!("cell {cell} out of range 1..={}", self.grid.cell_count())}),
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
                let q = args.get("question").and_then(Value::as_str).unwrap_or("Describe exactly what is on screen.");
                match read_view(self.view, q, ctx, cancel) {
                    Ok(answer) => {
                        eprintln!("[cc] step {step:02} LOOK: {answer}");
                        json!({"ok": true, "reading": answer})
                    }
                    Err(e) => json!({"ok": false, "error": format!("vision read failed: {e}")}),
                }
            }
            "click_target" => {
                let desc = args.get("description").and_then(Value::as_str).unwrap_or("");
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
                            append_click(&self.dir, json!({"step": step, "kind": "click_target", "desc": desc,
                                "button": button, "view_norm": [loc.x.round(), loc.y.round()],
                                "screen_px": [sx, sy], "saw": loc.note,
                                "view": [self.view.x, self.view.y, self.view.w, self.view.h]}));
                            eprintln!("[cc] step {step:02} CLICK_TARGET[{button}] '{desc}' -> screen({sx},{sy}) saw={:?}", loc.note);
                            let r = click_screen(sx, sy, self.dry, button, &self.profile, cancel);
                            json!({"ok": true, "located_view_norm": [loc.x, loc.y], "saw_at_target": loc.note, "click": r})
                        }
                        Err(e) => json!({"ok": false, "error": format!("could not locate '{desc}': {e}")}),
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
                    match (locate_in_view(self.view, from, ctx, cancel), locate_in_view(self.view, to, ctx, cancel)) {
                        (Ok(f), Ok(t)) => {
                            let (fsx, fsy) = self.view.to_screen_px(f.x, f.y);
                            let (tsx, tsy) = self.view.to_screen_px(t.x, t.y);
                            self.last_click = Some((tsx, tsy));
                            self.click_before = session::capture_region_fp(tsx, tsy, VC_HALF);
                            append_click(&self.dir, json!({"step": step, "kind": "drag_target", "from": from, "to": to,
                                "from_px": [fsx, fsy], "to_px": [tsx, tsy], "saw_from": f.note, "saw_to": t.note}));
                            eprintln!("[cc] step {step:02} DRAG_TARGET '{from}' -> '{to}' : ({fsx},{fsy})->({tsx},{tsy})");
                            let r = drag_screen(fsx, fsy, tsx, tsy, self.dry, &self.profile, cancel);
                            json!({"ok": true, "from": f.note, "to": t.note, "drag": r})
                        }
                        (Err(e), _) => json!({"ok": false, "error": format!("could not locate from '{from}': {e}")}),
                        (_, Err(e)) => json!({"ok": false, "error": format!("could not locate to '{to}': {e}")}),
                    }
                }
            }
            "point_at" => {
                // Same vision-locate as click_target, but MOVE the cursor onto the
                // target and stop - no click. For "point at / show me X" or to hover
                // and reveal a tooltip / hover-menu (dwell_seconds lets it surface).
                let desc = args.get("description").and_then(Value::as_str).unwrap_or("");
                let dwell = args.get("dwell_seconds").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 10.0);
                match locate_in_view(self.view, desc, ctx, cancel) {
                    Ok(loc) => {
                        let (sx, sy) = self.view.to_screen_px(loc.x, loc.y);
                        self.last_click = Some((sx, sy)); // mark where we pointed on the next frame
                        append_click(&self.dir, json!({"step": step, "kind": "point_at", "desc": desc,
                            "view_norm": [loc.x.round(), loc.y.round()], "screen_px": [sx, sy],
                            "saw": loc.note, "view": [self.view.x, self.view.y, self.view.w, self.view.h]}));
                        eprintln!("[cc] step {step:02} POINT_AT '{desc}' -> screen({sx},{sy}) saw={:?}", loc.note);
                        let r = point_screen(sx, sy, (dwell * 1000.0) as u64, self.dry, &self.profile, cancel);
                        json!({"ok": true, "pointed_view_norm": [loc.x, loc.y], "saw_at_target": loc.note, "move": r})
                    }
                    Err(e) => json!({"ok": false, "error": format!("could not point at '{desc}': {e}")}),
                }
            }
            "map_targets" => {
                let desc = args.get("description").and_then(Value::as_str).unwrap_or("");
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
                        eprintln!("[cc] step {step:02} MAP_TARGETS '{desc}' -> {} anchors", self.anchors.len());
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
                let anchor = self.anchors.get(id.wrapping_sub(1)).map(|(sx, sy, n)| (*sx, *sy, n.clone()));
                match anchor {
                    Some((sx, sy, note)) => {
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(&self.dir, json!({"step": step, "kind": "click_mark", "mark": id,
                            "button": button, "screen_px": [sx, sy], "saw": note}));
                        eprintln!("[cc] step {step:02} CLICK_MARK {id} -> screen({sx},{sy})");
                        let r = click_screen(sx, sy, self.dry, button, &self.profile, cancel);
                        json!({"ok": true, "clicked_mark": id, "what": note, "click": r})
                    }
                    None => json!({"ok": false, "error": format!("no anchor #{id} (have {}); run map_targets first", self.anchors.len())}),
                }
            }
            "wait" => {
                let secs = args.get("seconds").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.0, 30.0);
                let aborted = human_input::sleep_cancellable((secs * 1000.0) as u64, cancel);
                json!({"ok": !aborted, "waited_seconds": secs})
            }
            "type_text" | "key_combination" | "open_url" | "launch_app" | "run_command" | "click_here" => {
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    executor::execute_ex(name, args, &self.profile, cancel)
                }
            }
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
            "browser_query" => super::super::browser::query(args.get("selector").and_then(Value::as_str).unwrap_or("")),
            "browser_click" => super::super::browser::click_selector(args.get("selector").and_then(Value::as_str).unwrap_or("")),
            "browser_fill" => super::super::browser::fill(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("text").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_wait_for" => super::super::browser::wait_for(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("timeout_ms").and_then(Value::as_u64).unwrap_or(8000),
            ),
            "browser_eval" => super::super::browser::eval_js(args.get("code").and_then(Value::as_str).unwrap_or("")),
            "browser_navigate" => super::super::browser::navigate(args.get("url").and_then(Value::as_str).unwrap_or("")),
            "browser_open_tab" => super::super::browser::open_tab(args.get("url").and_then(Value::as_str).unwrap_or("")),
            "browser_upload" => super::super::browser::upload_file(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("path").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_tabs" => super::super::browser::get_tabs(),
            "browser_switch_tab" => super::super::browser::switch_tab(args.get("tab_id").and_then(Value::as_i64).unwrap_or(0)),
            "browser_network" => super::super::browser::read_network(args.get("filter").and_then(Value::as_str).unwrap_or("")),
            "decline_browser_control" => {
                super::super::browser::record_decline();
                json!({"ok": true, "noted": "won't ask again for a while"})
            }
            _ => json!({"ok": false, "error": "unknown action"}),
        };
        // Per-action latency (excludes the settle wait) — the key refinement
        // signal for vision/click cost. Full result is truncated to avoid bloat;
        // look()/click_target log their rich detail on their own lines above.
        let ms = t0.elapsed().as_millis();
        let settle = if name == "open_url" || name == "launch_app" { 1100 } else { 250 };
        std::thread::sleep(Duration::from_millis(settle));
        let short: String = result.to_string().chars().take(120).collect();
        eprintln!("[cc] step {step:02} {name} {ms}ms -> {short}");
        // Record the action trail (for situational context) + consecutive wait time.
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(true);
        self.trail.push(format!("{name}={}", if ok { "ok" } else { "fail" }));
        if self.trail.len() > 6 {
            self.trail.remove(0);
        }
        if name == "wait" {
            self.wait_accum += result.get("waited_seconds").and_then(Value::as_f64).unwrap_or(0.0);
        } else {
            self.wait_accum = 0.0;
        }
        result
    }

    /// Re-ground after an action: re-resolve the view (foreground-follow unless
    /// zoomed), render a marked frame, format state, and compute the #1 stuck +
    /// #2 state-delta notes.
    pub fn ground(&mut self, name: &str, args: &Value) -> Result<Grounded> {
        if !self.zoomed {
            self.view = window_view(self.target.as_deref(), self.whole_screen);
        }
        let (b, v, fp) = render_view(&self.dir, self.step, self.view, self.grid, self.last_click)?;
        self.view = v;
        // Informational tools don't change the screen; skip the heavy UIA readout
        // dump so their OWN result (memory transcript, clipboard text, window list)
        // is the dominant signal instead of being buried under hundreds of on-screen
        // elements — which made the agent answer from the SCREEN, not from memory.
        if matches!(
            name,
            "search_memory" | "open_memory" | "read_clipboard" | "list_windows" | "run_command"
            | "browser_setup" | "browser_status" | "browser_reset" | "browser_read_page"
            | "browser_query" | "browser_eval" | "browser_tabs" | "browser_network"
            | "decline_browser_control"
        ) {
            eprintln!("[cc] step {:02} (info tool — screen readouts suppressed)", self.step);
            return Ok(Grounded { frame_b64: b, state_text: self.context_block(), notes: Vec::new() });
        }
        let elements = uia::enumerate(self.target.as_deref()).unwrap_or_default();
        // Did the click change ITS OWN target cell? Compare the region snapshot
        // taken just before the click (`click_before`) to the same region now
        // (`fp`, fingerprinted around the click point). Localized, so a timer or
        // animation elsewhere doesn't fool it. Only set for click_at/click_target.
        let visual_no_change = match self.click_before.take() {
            Some(before) => session::fingerprint_change(&before, &fp) < vc_min(),
            None => false,
        };
        let ro = readouts_inline(&elements);
        let ro_short: String = ro.chars().take(220).collect();
        let more = if ro.chars().count() > 220 { " ..." } else { "" };
        eprintln!("[cc] step {:02} READOUTS ({} els): {ro_short}{more}", self.step, elements.len());
        let new_sig = state_signature(&elements);
        let ui_changed = self.prev_state_sig.as_deref() != Some(new_sig.as_str());
        self.prev_state_sig = Some(new_sig);
        let uia_action = matches!(name, "click_element" | "type_text" | "key_combination" | "open_url" | "launch_app");
        let act_sig = format!("{name}|{}", compact_args(args));
        self.recent_actions.push(act_sig.clone());
        if self.recent_actions.len() > 8 {
            self.recent_actions.remove(0);
        }
        // Repeating an action is only STUCK if nothing is changing. Paging through a
        // long page (scroll down, down, down) legitimately repeats while NEW content
        // keeps appearing - so gate on !ui_changed: a real loop (click not landing,
        // or scrolled to the very bottom) stops changing the accessible state, while
        // productive scrolling keeps changing it. Nav keys are exempt outright.
        let is_nav = name == "key_combination"
            && args.get("keys").and_then(Value::as_str).map(is_nav_keys).unwrap_or(false);
        let stuck = !is_nav
            && !ui_changed
            && self.recent_actions.iter().filter(|a| **a == act_sig).count() >= 3;
        let mut notes: Vec<(&'static str, &'static str)> = Vec::new();
        if visual_no_change {
            eprintln!("[cc] step {:02} NO VISUAL CHANGE after {name}", self.step);
            notes.push(("screen_change", "NONE - the visible screen did NOT change after this action, so it likely did NOT register (wrong target, the element isn't focused, or this surface ignores that input). Try a different approach - do not just repeat it."));
        }
        if uia_action && !ui_changed && !visual_no_change {
            notes.push(("ui_change", "none - the accessible UI did not change after this action; it may not have registered."));
        }
        if stuck {
            eprintln!("[cc] step {:02} STUCK: repeated '{act_sig}'", self.step);
            notes.push(("stuck_warning", "You have repeated the same action ~3 times with NOTHING changing. If you were scrolling, you have reached the end of the page/list - STOP scrolling and finish (answer the user or call done). Otherwise the target likely isn't where you think or the click isn't landing: change approach (zoom in, a more specific click_target, or read the page text directly)."));
        }
        let mut state = format!(
            "{}\n\n{}",
            self.context_block(),
            format_state(&elements, self.target.as_deref(), self.view, self.grid)
        );
        if let Some(marks) = self.detect_marks(&elements) {
            state.push_str(&format!("\n{marks}"));
        }
        Ok(Grounded { frame_b64: b, state_text: state, notes })
    }

    /// On a UIA-blind surface (canvas/game/custom-drawn) auto-run the local detector
    /// to mark clickable regions the accessibility tree can't see, populate
    /// `click_mark` anchors, and return a MARKS section for the state. `None` when not
    /// blind, no model installed, or nothing found. Throttled: only (re)detects while
    /// there are no anchors yet (zoom/reset_view clear them, re-triggering).
    fn detect_marks(&mut self, elements: &[UiElement]) -> Option<String> {
        if !super::super::detector::available() {
            return None;
        }
        // "Blind" = almost nothing actionable is visible in the current view.
        let visible = elements
            .iter()
            .filter(|e| {
                !e.name.trim().is_empty() && is_clickable(e.control_type) && {
                    let (cx, cy) = e.center();
                    cx >= self.view.x
                        && cx <= self.view.x + self.view.w
                        && cy >= self.view.y
                        && cy <= self.view.y + self.view.h
                }
            })
            .count();
        if visible > 2 {
            return None;
        }
        if self.anchors.is_empty() {
            self.anchors = super::super::detector::detect_view(self.view)
                .into_iter()
                .map(|b| (b.cx, b.cy, Some("clickable".to_string())))
                .collect();
        }
        if self.anchors.is_empty() {
            return None;
        }
        let mut s = String::from(
            "DETECTED CLICKABLE MARKS (this surface exposes NO UIA elements; these clickable \
regions were found visually - click_mark by its number; @cellN is where it sits):\n",
        );
        for (i, (sx, sy, note)) in self.anchors.iter().enumerate() {
            let mx = (*sx - self.view.x) as f64 / self.view.w.max(1) as f64 * 1000.0;
            let my = (*sy - self.view.y) as f64 / self.view.h.max(1) as f64 * 1000.0;
            let cell = if (0.0..=1000.0).contains(&mx) && (0.0..=1000.0).contains(&my) {
                format!("@cell{}", self.grid.cell_at(mx, my))
            } else {
                "@off-view".to_string()
            };
            let what = note.as_deref().unwrap_or("clickable");
            s.push_str(&format!("[{}] {what} {cell}\n", i + 1));
        }
        eprintln!("[cc] {} clickable marks on UIA-blind surface", self.anchors.len());
        Some(s)
    }
}
