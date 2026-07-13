//! The `Brain` impl — per-turn grounding, tool-call dispatch, and `done`
//! verification — split out of `uia_task.rs` for the file-size limit. `use
//! super::*` pulls in the shared imports, types, and render/vision helpers;
//! explicit `super::super::` paths reach the sibling CC modules.

use super::*;
impl Brain {
    pub(crate) fn bind_action(&mut self, action: super::super::telemetry::ActionTrace) {
        self.active_action = Some(action);
    }

    pub fn new(target: Option<String>) -> Self {
        // Frames, clicks and structured events share one session-scoped folder.
        // The session suffix is mandatory even when CC_TRACE_DIR is overridden.
        super::super::telemetry::begin_session();
        let trace_dir = super::super::telemetry::trace_dir();
        if let Err(error) = std::fs::create_dir_all(&trace_dir) {
            super::super::telemetry::artifact_write_failed(
                "trace_directory",
                &trace_dir,
                None,
                &error,
            );
        }
        let dir = trace_dir.to_string_lossy().into_owned();
        let view = window_view(target.as_deref(), false);
        let dry = std::env::var("CC_DRY").is_ok();
        let desktop = uia::virtual_desktop();
        super::super::telemetry::record_session_start(json!({
            "app_version": env!("CARGO_PKG_VERSION"),
            "live_model": super::super::protocol::MODEL,
            "dry_run": dry,
            "target": target,
            "trace_dir": dir,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "virtual_desktop": [desktop.0, desktop.1, desktop.2, desktop.3],
            "initial_view": [view.x, view.y, view.w, view.h],
            "executable": executable_provenance(),
        }));
        Self {
            dir,
            grid: Grid::from_env(),
            profile: HumanProfile::from_env(),
            dry,
            target,
            view,
            zoomed: false,
            whole_screen: false,
            last_click: None,
            step: 0,
            active_action: None,
            recent_actions: Vec::new(),
            prev_state_sig: None,
            click_before: None,
            trail: Vec::new(),
            wait_accum: 0.0,
            anchors: Vec::new(),
            controller: super::super::controller::Controller::new(),
            no_effect_strikes: 0,
            setup_guard: super::setup_guard::SetupGuard::default(),
        }
    }

    pub(super) fn finish_dispatch(
        &mut self,
        action: super::super::telemetry::ActionTrace,
        name: &str,
        args: &Value,
        result: Value,
        started: Instant,
    ) -> Value {
        self.setup_guard.record_result(name, &result);
        let dispatch_ms = started.elapsed().as_millis();
        let settle_ms = if matches!(name, "open_url" | "launch_app") {
            1100
        } else {
            250
        };
        std::thread::sleep(Duration::from_millis(settle_ms));
        let total_ms = started.elapsed().as_millis();
        let short = match name {
            "observe" => format!(
                "{} elements",
                result.get("count").and_then(Value::as_u64).unwrap_or(0)
            ),
            "act" => {
                let id = args.get("id").and_then(Value::as_u64).unwrap_or(0);
                let verb = args.get("verb").and_then(Value::as_str).unwrap_or("act");
                let target = result
                    .pointer("/target/name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let outcome = result
                    .get("verify")
                    .and_then(Value::as_str)
                    .or_else(|| result.get("blocked").and_then(Value::as_str))
                    .or_else(|| result.get("error").and_then(Value::as_str))
                    .unwrap_or("ok");
                format!(
                    "{verb} @{id} {target:?} -> {}",
                    outcome.chars().take(110).collect::<String>()
                )
            }
            "click_at" | "click_target" | "click_mark" | "point_at" => format!(
                "ok={} view_norm={} screen_px={} view_rect={}",
                result.get("ok").unwrap_or(&Value::Null),
                result.get("view_norm").unwrap_or(&Value::Null),
                result.get("screen_px").unwrap_or(&Value::Null),
                result.get("view_rect").unwrap_or(&Value::Null),
            ),
            "wait" => {
                let seconds = result
                    .get("waited_seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                format!(
                    "{seconds:.0}s (~{:.0}s total waiting; if nothing's changing, STOP)",
                    self.wait_accum + seconds
                )
            }
            _ => result.to_string().chars().take(120).collect(),
        };
        super::super::telemetry::human(
            "cc",
            format!("step {:02} {name} {total_ms}ms -> {short}", self.step),
        );
        super::super::telemetry::tool_result(
            action,
            name,
            self.step,
            total_ms,
            result.get("ok").and_then(Value::as_bool),
            json!({
                "result_preview": short,
                "blocked": result.get("blocked"),
                "error": result.get("error"),
                "code": result.get("code"),
                "input_injection": super::super::executor::input_injection(&result),
                "timing": {
                    "dispatch_ms": dispatch_ms,
                    "settle_ms": settle_ms,
                    "total_ms": total_ms,
                },
                "coordinates": {
                    "view_norm": result.get("view_norm"),
                    "screen_px": result.get("screen_px"),
                    "view_rect": result.get("view_rect"),
                    "coordinate_spaces": result.get("coordinate_spaces"),
                },
            }),
        );
        if !matches!(name, "observe" | "act") {
            self.controller.invalidate();
        }
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(true);
        self.trail
            .push(format!("{name}={}", if ok { "ok" } else { "fail" }));
        if self.trail.len() > 6 {
            self.trail.remove(0);
        }
        self.wait_accum = if name == "wait" {
            self.wait_accum
                + result
                    .get("waited_seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
        } else {
            0.0
        };
        result
    }

    /// Per-turn grounding context the model gets above the element list: where it
    /// is (window), where the cursor is + what's under it, what it just did, and
    /// how long it's been waiting. Cheap situational awareness.
    fn context_block(&self) -> String {
        let (title, cx, cy) = uia::pointer_context();
        let title: String = if title.is_empty() {
            "(unknown)".into()
        } else {
            title.chars().take(70).collect()
        };
        let trail = if self.trail.is_empty() {
            "(none yet)".to_string()
        } else {
            self.trail.join("  |  ")
        };
        let mut s =
            format!("Active window: {title}\nCursor at ({cx},{cy})\nYour recent actions: {trail}");
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
        let semantic = self.semantic_surface_state();
        let elements = if semantic.is_none() {
            uia::enumerate(self.target.as_deref()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let (b, v, _fp, _frame_id) = render_view(
            &self.dir, self.step, self.view, self.grid, None, "initial", None,
        )?;
        self.view = v;
        self.prev_state_sig = Some(
            semantic
                .as_ref()
                .map(|state| format!("structured:{state}"))
                .unwrap_or_else(|| state_signature(&elements)),
        );
        let mut state = semantic.unwrap_or_else(|| {
            format_state(&elements, self.target.as_deref(), self.view, self.grid)
        });
        if !elements.is_empty()
            && let Some(marks) = self.detect_marks(&elements)
        {
            state.push_str(&format!("\n{marks}"));
        }
        Ok((b, state))
    }

    /// Re-ground after an action: re-resolve the view (foreground-follow unless
    /// zoomed), render a marked frame, format state, and compute the #1 stuck +
    /// #2 state-delta notes.
    pub fn ground(&mut self, name: &str, args: &Value) -> Result<Grounded> {
        if !self.zoomed {
            self.view = window_view(self.target.as_deref(), self.whole_screen);
        }
        let action = self.active_action.take();
        let (b, v, fp, frame_id) = render_view(
            &self.dir,
            self.step,
            self.view,
            self.grid,
            self.last_click,
            name,
            action,
        )?;
        self.view = v;
        // Informational tools don't change the screen; skip the heavy UIA readout
        // dump so their OWN result (memory transcript, clipboard text, window list)
        // is the dominant signal instead of being buried under hundreds of on-screen
        // elements — which made the agent answer from the SCREEN, not from memory.
        if matches!(
            name,
            "observe"
                | "act"
                | "do_steps"
                | "search_memory"
                | "open_memory"
                | "read_clipboard"
                | "list_windows"
                | "system_query"
                | "run_command"
                | "browser_setup"
                | "browser_status"
                | "browser_reset"
                | "browser_read_page"
                | "research_web"
                | "browser_eval"
                | "browser_tabs"
                | "browser_network"
                | "browser_console"
                | "decline_browser_control"
                | "list_app_integrations"
                | "setup_app_integration"
                | "app_integration_status"
                | "read_app_integration_docs"
                | "remove_app_integration"
                | "decline_app_integration"
        ) {
            eprintln!(
                "[cc] step {:02} (info tool; screen readouts suppressed)",
                self.step
            );
            return Ok(Grounded {
                frame_b64: b,
                frame_id,
                state_text: self.context_block(),
                notes: Vec::new(),
            });
        }
        let semantic = self.semantic_surface_state();
        let elements = if semantic.is_none() {
            uia::enumerate(self.target.as_deref()).unwrap_or_default()
        } else {
            Vec::new()
        };
        // Did the click change ITS OWN target cell? Compare the region snapshot
        // taken just before the click (`click_before`) to the same region now
        // (`fp`, fingerprinted around the click point). Localized, so a timer or
        // animation elsewhere doesn't fool it. Only set for click_at/click_target.
        let visual_no_change = match self.click_before.take() {
            Some(before) => session::fingerprint_change(&before, &fp) < vc_min(),
            None => false,
        };
        if let Some(state) = &semantic {
            super::super::telemetry::event(
                "semantic_surface_observed",
                "grounding",
                super::super::telemetry::Privacy::Safe,
                json!({
                    "provider": "browser_bridge",
                    "state_preview": state.chars().take(240).collect::<String>(),
                    "input_target": uia::input_target_snapshot(),
                }),
            );
        } else {
            let ro = readouts_inline(&elements);
            let ro_short: String = ro.chars().take(220).collect();
            let more = if ro.chars().count() > 220 { " ..." } else { "" };
            eprintln!(
                "[cc] step {:02} READOUTS ({} els): {ro_short}{more}",
                self.step,
                elements.len()
            );
        }
        let new_sig = semantic
            .as_ref()
            .map(|state| format!("structured:{state}"))
            .unwrap_or_else(|| state_signature(&elements));
        let ui_changed = self.prev_state_sig.as_deref() != Some(new_sig.as_str());
        self.prev_state_sig = Some(new_sig);
        let uia_action = matches!(
            name,
            "type_text" | "key_combination" | "open_url" | "launch_app"
        );
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
            && args
                .get("keys")
                .and_then(Value::as_str)
                .map(is_nav_keys)
                .unwrap_or(false);
        let stuck = !is_nav
            && !ui_changed
            && self
                .recent_actions
                .iter()
                .filter(|a| **a == act_sig)
                .count()
                >= 3;
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
        let no_effect = notes
            .iter()
            .any(|(k, _)| matches!(*k, "screen_change" | "ui_change" | "stuck_warning"));
        if no_effect {
            self.no_effect_strikes += 1;
            notes.push((
                "postcondition",
                "This action produced no confirmed effect. Re-observe/replan or change tool family; do not repeat the same action.",
            ));
            if self.no_effect_strikes >= 2 {
                notes.push((
                    "postcondition_block",
                    "Repeated no-effect actions detected. Stop retrying; use a different route or explain the concrete blocker.",
                ));
            }
        } else {
            self.no_effect_strikes = 0;
        }
        self.setup_guard.after_ground(&notes);
        if let Some(note) = self.setup_guard.note() {
            notes.push(note);
        }
        let surface_state = semantic.unwrap_or_else(|| {
            format_state(&elements, self.target.as_deref(), self.view, self.grid)
        });
        let mut state = format!("{}\n\n{}", self.context_block(), surface_state);
        if !elements.is_empty()
            && let Some(marks) = self.detect_marks(&elements)
        {
            state.push_str(&format!("\n{marks}"));
        }
        Ok(Grounded {
            frame_b64: b,
            frame_id,
            state_text: state,
            notes,
        })
    }

    fn semantic_surface_state(&mut self) -> Option<String> {
        if !super::super::browser::input_active() {
            return None;
        }
        let observed = self.controller.observe();
        if observed.get("ok").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let elements = observed.get("elements").and_then(Value::as_str)?;
        let title = observed.get("title").and_then(Value::as_str).unwrap_or("");
        let url = observed.get("url").and_then(Value::as_str).unwrap_or("");
        super::super::telemetry::human(
            "cc",
            format!(
                "semantic provider=browser_bridge title={:?} url={:?}",
                title.chars().take(70).collect::<String>(),
                url.chars().take(100).collect::<String>()
            ),
        );
        Some(elements.to_string())
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
                .map(|b| (b.cx, b.cy, Some("clickable".to_string()), None))
                .collect();
        }
        if self.anchors.is_empty() {
            return None;
        }
        let mut s = String::from(
            "DETECTED CLICKABLE MARKS (this surface exposes NO UIA elements; these clickable \
regions were found visually - click_mark by its number; @cellN is where it sits):\n",
        );
        for (i, (sx, sy, note, _)) in self.anchors.iter().enumerate() {
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
        eprintln!(
            "[cc] {} clickable marks on UIA-blind surface",
            self.anchors.len()
        );
        Some(s)
    }
}

fn executable_provenance() -> Value {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let Ok(path) = std::env::current_exe() else {
        return json!({"error": "current executable path unavailable"});
    };
    let metadata = std::fs::metadata(&path).ok();
    let modified_ms = metadata
        .as_ref()
        .and_then(|value| value.modified().ok())
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis());
    let sha256 = std::fs::File::open(&path).ok().and_then(|mut file| {
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        Some(format!("{:x}", hasher.finalize()))
    });
    json!({
        "path": path,
        "byte_count": metadata.map(|value| value.len()),
        "modified_unix_ms": modified_ms,
        "sha256": sha256,
    })
}
