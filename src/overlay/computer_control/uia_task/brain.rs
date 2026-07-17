//! Per-turn grounding, dispatch finalization, and evidence state for `Brain`.

use super::*;
impl Brain {
    pub(crate) fn begin_job(
        &mut self,
        turn_id: u64,
        source_frame: Option<FrameSource>,
        inherit_evidence: bool,
    ) {
        self.setup_guard.begin_turn(turn_id);
        if self.current_turn_id == Some(turn_id) {
            if let Some(FrameSource {
                surface:
                    super::super::controller::world::SurfaceIdentity::Browser {
                        tab_id,
                        document_id,
                        ..
                    },
                ..
            }) = source_frame.as_ref()
                && self.controlled_tab_id == Some(*tab_id)
            {
                self.controlled_document_id = Some(document_id.clone());
            }
            self.source_frame = source_frame;
            return;
        }
        if self.current_turn_id.is_some() {
            self.retire_owned_tabs(super::tab_ownership::RetirementReason::Superseded);
        }
        self.current_turn_id = Some(turn_id);
        let browser_source = source_frame
            .as_ref()
            .and_then(|source| match &source.surface {
                super::super::controller::world::SurfaceIdentity::Browser {
                    tab_id,
                    document_id,
                    ..
                } => Some((*tab_id, document_id.clone())),
                super::super::controller::world::SurfaceIdentity::Native { .. } => None,
            });
        self.controlled_tab_id = browser_source.as_ref().map(|source| source.0);
        self.controlled_document_id = browser_source.map(|source| source.1);
        self.source_frame = source_frame;
        self.active_action = None;
        self.recent_actions.clear();
        self.advice_latches.clear();
        self.prev_state_sig = None;
        self.trail.clear();
        self.exact_edit_guard.reset();
        self.resource_authorization
            .begin_turn(turn_id, inherit_evidence);
        self.structural_authorization
            .begin_turn(turn_id, inherit_evidence);
        self.wait_accum = 0.0;
        self.last_click = None;
        self.click_before = None;
        self.zoomed = false;
        self.whole_screen = false;
        self.view = window_view(self.target.as_deref(), false);
        self.show_coarse_grid = false;
        self.controller
            .bind_source_surface(self.source_frame.as_ref().map(|source| &source.surface));
        self.clear_anchors("new_turn");
    }

    pub(crate) fn retire_turn(&mut self, turn_id: u64) {
        if self.current_turn_id == Some(turn_id) {
            self.retire_owned_tabs(super::tab_ownership::RetirementReason::Completed);
            self.clear_retired_turn_state();
        }
    }

    pub(crate) fn interrupt_turn(&mut self, turn_id: u64) {
        if self.current_turn_id == Some(turn_id) {
            self.retire_owned_tabs(super::tab_ownership::RetirementReason::Interrupted);
            self.clear_retired_turn_state();
        }
    }

    pub(crate) fn retire_session_completed(&mut self) {
        self.retire_owned_tabs(super::tab_ownership::RetirementReason::Completed);
        self.clear_retired_turn_state();
    }

    pub(super) fn retire_owned_tabs(&mut self, reason: super::tab_ownership::RetirementReason) {
        self.turn_tabs
            .retire(self.current_turn_id, reason)
            .record_telemetry();
    }

    fn clear_retired_turn_state(&mut self) {
        self.active_action = None;
        self.controlled_tab_id = None;
        self.controlled_document_id = None;
        self.source_frame = None;
        self.current_turn_id = None;
        self.controller.release_turn_target();
        self.setup_guard.retire();
        self.exact_edit_guard.reset();
        self.keyboard_target_gate.reset();
    }

    pub fn new(target: Option<String>) -> Self {
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
        let requested_target = target.clone();
        let view = window_view(target.as_deref(), false);
        let dry = super::harness_options::dry_run_requested();
        let desktop = uia::virtual_desktop();
        super::super::telemetry::record_session_start(json!({
            "app_version": env!("CARGO_PKG_VERSION"),
            "live_model": super::super::protocol::MODEL,
            "dry_run": dry,
            "target": requested_target,
            "pinned_target": target,
            "trace_dir": dir,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "virtual_desktop": [desktop.0, desktop.1, desktop.2, desktop.3],
            "initial_view": [view.x, view.y, view.w, view.h],
            "executable": super::executable_provenance::capture(),
        }));
        let controller = super::super::controller::Controller::new(target.clone());
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
            current_turn_id: None,
            source_frame: None,
            controlled_tab_id: None,
            controlled_document_id: None,
            turn_tabs: super::tab_ownership::TurnTabOwnership::default(),
            recent_actions: Vec::new(),
            advice_latches: Vec::new(),
            prev_state_sig: None,
            click_before: None,
            trail: Vec::new(),
            exact_edit_guard: super::exact_edit_guard::ExactEditGuard::default(),
            structural_authorization:
                super::structural_authorization::StructuralAuthorization::default(),
            resource_authorization: super::resource_authorization::ResourceAuthorization::default(),
            wait_accum: 0.0,
            anchors: Vec::new(),
            next_anchor_id: 1,
            controller,
            show_coarse_grid: false,
            setup_guard: super::setup_guard::SetupGuard::default(),
            keyboard_target_gate: super::keyboard_target_gate::KeyboardTargetGate::default(),
        }
    }

    pub(super) fn finish_dispatch(
        &mut self,
        action: super::super::telemetry::ActionTrace,
        name: &str,
        args: &Value,
        mut result: Value,
        started: Instant,
    ) -> Value {
        self.setup_guard.record_result(name, &result);
        self.exact_edit_guard.record_result(name, args, &mut result);
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
                "failure": dispatch_telemetry::structural_failure(&result),
                "response": dispatch_telemetry::response_size(&result),
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
        if !matches!(name, "observe" | "act" | "do_steps") {
            self.controller.invalidate();
        }
        super::receipts::push_result(&mut self.trail, name, &result);
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
        let keyboard_target =
            uia::foreground_stable_target().unwrap_or_else(|| "(unknown)".to_string());
        let mut s = format!(
            "Active window: {title}\nKeyboard target: {keyboard_target}\nCursor at ({cx},{cy})\nYour recent actions: {trail}"
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
        let semantic = self.semantic_surface_state();
        let has_semantic_surface = semantic.is_some();
        let native = (semantic.is_none() && !super::super::browser::input_active())
            .then(|| native_perception(self.target.as_deref()));
        let elements = native
            .as_ref()
            .map(|perception| perception.elements.clone())
            .unwrap_or_default();
        let accessibility_observed = native.as_ref().is_none_or(|state| state.observed);
        let perception_surface = semantic
            .as_ref()
            .map(|state| &state.identity)
            .or_else(|| native.as_ref().and_then(|state| state.surface.as_ref()));
        let detector_start_id = (!has_semantic_surface
            && self.anchors.is_empty()
            && current_surface_identity(self.target.as_deref()).is_some()
            && detector_surface_blind(&elements, self.view)
            && super::super::detector::available())
        .then_some(self.next_anchor_id);
        let excluded = accessible_rects(&elements, self.view);
        self.show_coarse_grid =
            !has_semantic_surface && accessibility_observed && excluded.is_empty();
        let existing_marks = self.anchor_marks();
        let Rendered {
            frame_b64: b,
            view: v,
            fingerprint: _fp,
            frame_id,
            surface,
            source,
            fixed_view_retained: _,
            perception_matched,
            detected,
        } = render_view(RenderRequest {
            dir: &self.dir,
            target: self.target.as_deref(),
            step: self.step,
            view: self.view,
            whole_screen: false,
            preserve_view: false,
            bound_source: None,
            perception_surface,
            grid: self.grid,
            marker: None,
            reason: "initial",
            action: None,
            existing_marks: &existing_marks,
            detector_start_id,
            excluded_rects: &excluded,
            show_grid: self.show_coarse_grid,
        })?;
        if !perception_matched {
            anyhow::bail!("surface changed between initial perception and frame capture");
        }
        self.view = v;
        self.source_frame = Some(source);
        if detector_start_id.is_some() {
            self.install_detector_anchors(detected, frame_id, v, surface);
        }
        self.prev_state_sig = Some(
            semantic
                .as_ref()
                .map(|state| format!("structured:{}", state.elements))
                .unwrap_or_else(|| state_signature(&elements)),
        );
        let indexed = semantic.is_none().then(|| self.controller.prime_native());
        let mut state = semantic.map(|state| state.elements).unwrap_or_else(|| {
            format_state(
                &elements,
                self.target.as_deref(),
                self.view,
                self.grid,
                self.show_coarse_grid,
                indexed.as_deref(),
            )
        });
        if let Some(marks) = self.marks_state() {
            state.push_str(&format!("\n{marks}"));
        }
        Ok((b, state))
    }

    /// Re-ground and derive one typed visual/accessibility postcondition.
    pub fn ground(&mut self, name: &str, args: &Value) -> Result<Grounded> {
        let action = self.active_action.take();
        let suppress_readouts = !super::super::turn_policy::is_mutating_tool(name);
        let semantic = self.semantic_surface_state();
        let (elements, accessibility_observed, perception_surface) = if suppress_readouts {
            (
                Vec::new(),
                true,
                semantic.as_ref().map(|state| state.identity.clone()),
            )
        } else {
            let native = (semantic.is_none() && !super::super::browser::input_active())
                .then(|| native_perception(self.target.as_deref()));
            let elements = native
                .as_ref()
                .map(|perception| perception.elements.clone())
                .unwrap_or_default();
            let observed = native.as_ref().is_none_or(|state| state.observed);
            let surface = semantic
                .as_ref()
                .map(|state| state.identity.clone())
                .or_else(|| native.as_ref().and_then(|state| state.surface.clone()));
            (elements, observed, surface)
        };
        self.invalidate_bound_anchors_for_new_frame();
        let detector_start_id = (!suppress_readouts
            && semantic.is_none()
            && self.anchors.is_empty()
            && current_surface_identity(self.target.as_deref()).is_some()
            && detector_surface_blind(&elements, self.view)
            && super::super::detector::available())
        .then_some(self.next_anchor_id);
        let excluded = accessible_rects(&elements, self.view);
        if !suppress_readouts {
            self.show_coarse_grid =
                semantic.is_none() && accessibility_observed && excluded.is_empty();
        }
        let existing_marks = self.anchor_marks();
        let Rendered {
            frame_b64: b,
            view: v,
            fingerprint: fp,
            frame_id,
            surface,
            source,
            fixed_view_retained,
            perception_matched,
            detected,
        } = render_view(RenderRequest {
            dir: &self.dir,
            target: self.target.as_deref(),
            step: self.step,
            view: self.view,
            whole_screen: self.whole_screen,
            preserve_view: self.zoomed,
            bound_source: self.source_frame.as_ref(),
            perception_surface: perception_surface.as_ref(),
            grid: self.grid,
            marker: self.last_click,
            reason: name,
            action,
            existing_marks: &existing_marks,
            detector_start_id,
            excluded_rects: &excluded,
            show_grid: self.show_coarse_grid,
        })?;
        if perception_surface.is_some() && !perception_matched {
            super::super::telemetry::typed_error(
                "ERR_FRAME_PERCEPTION_SURFACE_CHANGED",
                "grounding",
                "surface changed between perception and frame capture",
                json!({
                    "frame_source": source,
                    "perception_surface": perception_surface,
                    "tool": name,
                }),
            );
            anyhow::bail!("surface changed between perception and frame capture");
        }
        self.view = v;
        self.source_frame = Some(source.clone());
        if self.zoomed && !fixed_view_retained {
            self.zoomed = false;
            self.clear_anchors("zoomed_surface_changed");
        }
        if detector_start_id.is_some() {
            self.install_detector_anchors(detected, frame_id, v, surface);
        } else if !self.anchors.is_empty() {
            self.bind_pending_anchors(frame_id, v, surface);
        }
        if suppress_readouts {
            eprintln!(
                "[cc] step {:02} (info tool; screen readouts suppressed)",
                self.step
            );
            return Ok(Grounded {
                frame_b64: b,
                source,
                state_text: self.context_block(),
                postcondition: GroundPostcondition::default(),
            });
        }
        // Compare the click's own pre/post region so unrelated animation cannot
        // fake an effect. Only click_at/click_target sets this snapshot.
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
                    "title": state.title,
                    "url": state.url,
                    "identity": state.identity,
                    "state_preview": state.elements.chars().take(240).collect::<String>(),
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
            .map(|state| format!("structured:{}", state.elements))
            .unwrap_or_else(|| state_signature(&elements));
        let ui_changed = self.prev_state_sig.as_deref() != Some(new_sig.as_str());
        self.prev_state_sig = Some(new_sig.clone());
        let act_sig = record_action(&mut self.recent_actions, name, args);
        // Repetition is stuck only when state is unchanged; navigation may reveal new content.
        let is_nav = name == "key_combination"
            && args
                .get("keys")
                .and_then(Value::as_str)
                .map(is_nav_keys)
                .unwrap_or(false);
        let stuck = is_repeated_unchanged(&self.recent_actions, &act_sig, ui_changed, is_nav);
        if visual_no_change {
            eprintln!("[cc] step {:02} NO VISUAL CHANGE after {name}", self.step);
        }
        if stuck {
            eprintln!("[cc] step {:02} STUCK: repeated '{act_sig}'", self.step);
        }
        let postcondition = if stuck {
            let request_advice = latch_advice(&mut self.advice_latches, &act_sig, &new_sig);
            GroundPostcondition::no_effect(
                NoEffectReason::RepeatedUnchangedState,
                true,
                request_advice,
            )
        } else {
            GroundPostcondition::default()
        };
        let indexed = semantic.is_none().then(|| self.controller.prime_native());
        let surface_state = semantic.map(|state| state.elements).unwrap_or_else(|| {
            format_state(
                &elements,
                self.target.as_deref(),
                self.view,
                self.grid,
                self.show_coarse_grid,
                indexed.as_deref(),
            )
        });
        let mut state = format!("{}\n\n{}", self.context_block(), surface_state);
        if let Some(marks) = self.marks_state() {
            state.push_str(&format!("\n{marks}"));
        }
        Ok(Grounded {
            frame_b64: b,
            source,
            state_text: state,
            postcondition,
        })
    }
}
