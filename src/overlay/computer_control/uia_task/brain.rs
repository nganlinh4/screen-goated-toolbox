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
            controller: super::super::controller::Controller::new(),
            no_effect_strikes: 0,
            setup_guard: super::setup_guard::SetupGuard::default(),
        }
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
                "[cc] step {:02} (info tool — screen readouts suppressed)",
                self.step
            );
            return Ok(Grounded {
                frame_b64: b,
                state_text: self.context_block(),
                notes: Vec::new(),
            });
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
        eprintln!(
            "[cc] step {:02} READOUTS ({} els): {ro_short}{more}",
            self.step,
            elements.len()
        );
        let new_sig = state_signature(&elements);
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
        let mut state = format!(
            "{}\n\n{}",
            self.context_block(),
            format_state(&elements, self.target.as_deref(), self.view, self.grid)
        );
        if let Some(marks) = self.detect_marks(&elements) {
            state.push_str(&format!("\n{marks}"));
        }
        Ok(Grounded {
            frame_b64: b,
            state_text: state,
            notes,
        })
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
        eprintln!(
            "[cc] {} clickable marks on UIA-blind surface",
            self.anchors.len()
        );
        Some(s)
    }
}
