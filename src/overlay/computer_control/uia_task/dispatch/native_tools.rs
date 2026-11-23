//! Native input, filesystem, process, and window tool dispatch.

use super::super::*;

impl Brain {
    #[inline(never)]
    pub(super) fn dispatch_native_tool(
        &mut self,
        name: &str,
        args: &Value,
        cancel: &Arc<AtomicBool>,
        action: super::super::super::telemetry::ActionTrace,
        authorize_repair_process: bool,
    ) -> Option<Value> {
        Some(match name {
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
            "paste_artifact" => {
                if self.dry {
                    super::super::super::artifacts::paste_tool(args, &self.profile, cancel, true)
                } else {
                    match guarded_artifact_paste_args(
                        args.clone(),
                        self.target.as_deref(),
                        self.source_frame.as_ref(),
                        self.keyboard_target_gate.refocus_required(),
                    ) {
                        Ok(guarded) => super::super::super::artifacts::paste_tool(
                            &guarded,
                            &self.profile,
                            cancel,
                            false,
                        ),
                        Err(error) => dispatch_telemetry::pre_dispatch_failure(error),
                    }
                }
            }
            "system_query" => super::super::super::system_query::query(args),
            "list_files" => super::super::super::system_query::list_files(args),
            "read_text_file" => executor::execute_ex(name, args, &self.profile, cancel),
            "scroll" => self.dispatch_scroll(args, cancel),
            "drag" => self.dispatch_grid_drag(args, cancel),
            "focus_window" => window_admission(cancel, || self.dispatch_focus_window(args)),
            "list_windows" => {
                json!({"ok": true, "windows": super::super::super::uia::list_windows()})
            }
            "read_clipboard" => match super::super::super::clipboard::get_text_for_query() {
                Ok(text) => json!({"ok": true, "text": text}),
                Err(error) => {
                    json!({"ok": false, "error_code": error.code(), "error": error.message()})
                }
            },
            "minimize_window" => {
                window_admission(cancel, || self.dispatch_minimize_window(args, cancel))
            }
            "resize_window" => {
                window_admission(cancel, || self.dispatch_resize_window(args, cancel))
            }
            "move_window" => window_admission(cancel, || self.dispatch_move_window(args, cancel)),
            _ => return None,
        })
    }

    fn dispatch_scroll(&self, args: &Value, cancel: &Arc<AtomicBool>) -> Value {
        let (mx, my) = super::index_arg(args, "cell")
            .and_then(|cell| self.grid.center_norm(cell))
            .unwrap_or((500.0, 500.0));
        let (screen_x, screen_y) = self.view.to_screen_px(mx, my);
        let (x, y) = executor::screen_to_norm(screen_x, screen_y);
        let guarded_args = json!({
            "x": x,
            "y": y,
            "direction": args.get("direction").and_then(Value::as_str).unwrap_or("down"),
            "magnitude": args.get("amount").and_then(Value::as_f64).unwrap_or(5.0),
        });
        self.execute_guarded_pointer("scroll", guarded_args, cancel)
    }

    fn dispatch_grid_drag(&self, args: &Value, cancel: &Arc<AtomicBool>) -> Value {
        let from = super::index_arg(args, "from_cell").and_then(|cell| self.grid.center_norm(cell));
        let to = super::index_arg(args, "to_cell").and_then(|cell| self.grid.center_norm(cell));
        let (Some((from_x, from_y)), Some((to_x, to_y))) = (from, to) else {
            return json!({"ok": false, "error": "drag needs from_cell and to_cell"});
        };
        let (from_screen_x, from_screen_y) = self.view.to_screen_px(from_x, from_y);
        let (to_screen_x, to_screen_y) = self.view.to_screen_px(to_x, to_y);
        let (x, y) = executor::screen_to_norm(from_screen_x, from_screen_y);
        let (dest_x, dest_y) = executor::screen_to_norm(to_screen_x, to_screen_y);
        self.execute_guarded_pointer(
            "drag",
            json!({"x": x, "y": y, "dest_x": dest_x, "dest_y": dest_y}),
            cancel,
        )
    }

    fn execute_guarded_pointer(&self, name: &str, args: Value, cancel: &Arc<AtomicBool>) -> Value {
        if self.dry {
            return json!({"ok": true, "note": "dry"});
        }
        match guarded_input_args(args, self.target.as_deref(), self.source_frame.as_ref()) {
            Ok(guarded) => executor::execute_ex(name, &guarded, &self.profile, cancel),
            Err(error) => dispatch_telemetry::pre_dispatch_failure(error),
        }
    }

    fn dispatch_focus_window(&mut self, args: &Value) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        self.keyboard_target_gate.begin_focus_attempt();
        match super::super::super::uia::raise_window_with_target(title) {
            Err(error) => window_error(error),
            Ok((raised, target)) => {
                self.keyboard_target_gate.record_focus_result(raised);
                std::thread::sleep(Duration::from_millis(200));
                if raised {
                    self.whole_screen = false;
                    self.zoomed = false;
                    self.clear_anchors("focused_different_window");
                }
                let foreground = super::super::super::uia::pointer_context().0;
                json!({
                    "ok": raised,
                    "target": target,
                    "foreground_now": foreground,
                    "effect_verified": raised,
                    "effect_may_have_occurred": true,
                    "executed": raised.then_some(true),
                    "note": if raised { "switched" } else { "BLOCKED: the resolved window did not become foreground. Do not repeat the same focus attempt blindly; use a non-foreground provider when one exposes the needed state, otherwise report the blocker." }
                })
            }
        }
    }

    fn dispatch_minimize_window(&self, args: &Value, cancel: &AtomicBool) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        match super::super::super::uia::minimize_window(title, cancel) {
            Err(error) => window_error(error),
            Ok(ok) => {
                std::thread::sleep(Duration::from_millis(200));
                json!({"ok": ok, "foreground_now": super::super::super::uia::pointer_context().0})
            }
        }
    }

    fn dispatch_resize_window(&self, args: &Value, cancel: &AtomicBool) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        let (width, height) = match window_pair(args, "width", "height") {
            Ok(pair) => pair,
            Err(error) => return error,
        };
        super::super::super::uia::resize_window(title, width, height, cancel)
            .map(|ok| json!({"ok": ok}))
            .unwrap_or_else(window_error)
    }

    fn dispatch_move_window(&self, args: &Value, cancel: &AtomicBool) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        let (x, y) = match window_pair(args, "x", "y") {
            Ok(pair) => pair,
            Err(error) => return error,
        };
        super::super::super::uia::move_window(title, x, y, cancel)
            .map(|ok| json!({"ok": ok}))
            .unwrap_or_else(window_error)
    }
}

fn window_admission(cancel: &AtomicBool, action: impl FnOnce() -> Value) -> Value {
    if executor::with_job_admission(Some(cancel), || ()).is_none() {
        return json!({"ok": false, "code": "cancelled", "effect_may_have_occurred": false});
    }
    // An operation admitted before cancellation can still finish afterward.
    action()
}

fn window_pair(args: &Value, first: &str, second: &str) -> Result<(i32, i32), Value> {
    let parse = |name: &str| {
        args.get(name)
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| {
                json!({"ok": false, "code": "invalid_window_argument", "error": format!("{name} must be a 32-bit integer"), "effect_may_have_occurred": false})
            })
    };
    Ok((parse(first)?, parse(second)?))
}

fn window_error(error: super::super::super::uia::WindowError) -> Value {
    let effect_may_have_occurred = match &error {
        super::super::super::uia::WindowError::Cancelled {
            effect_may_have_occurred,
        } => Some(*effect_may_have_occurred),
        super::super::super::uia::WindowError::Unconfirmed { .. } => Some(true),
        super::super::super::uia::WindowError::IneligibleTarget { .. } => Some(false),
        _ => None,
    };
    json!({
        "ok": false,
        "code": error.code(),
        "error": error.to_string(),
        "effect_may_have_occurred": effect_may_have_occurred,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ineligible_window_reports_no_effect() {
        let receipt = window_error(
            super::super::super::super::uia::WindowError::IneligibleTarget {
                hwnd: 10,
                reason: "not_top_level",
            },
        );
        assert_eq!(receipt["code"], "ineligible_window_target");
        assert_eq!(receipt["effect_may_have_occurred"], false);
    }

    #[test]
    fn window_coordinates_reject_missing_fractional_and_out_of_range_values() {
        assert_eq!(
            window_pair(&json!({"x": i32::MIN, "y": i32::MAX}), "x", "y"),
            Ok((i32::MIN, i32::MAX))
        );
        for value in [
            json!(i64::from(i32::MAX) + 1),
            json!(i64::from(i32::MIN) - 1),
            json!(1.5),
            json!(null),
        ] {
            let error = window_pair(&json!({"x": value, "y": 0}), "x", "y").unwrap_err();
            assert_eq!(error["code"], "invalid_window_argument");
            assert_eq!(error["effect_may_have_occurred"], false);
        }
        assert!(window_pair(&json!({"x": 0}), "x", "y").is_err());
    }

    #[test]
    fn cancellation_receipt_preserves_whether_restore_was_admitted() {
        for may_have_occurred in [false, true] {
            let receipt = window_error(super::super::super::super::uia::WindowError::Cancelled {
                effect_may_have_occurred: may_have_occurred,
            });
            assert_eq!(receipt["code"], "cancelled");
            assert_eq!(receipt["effect_may_have_occurred"], may_have_occurred);
        }
    }

    #[test]
    fn unconfirmed_window_request_reports_a_possible_effect() {
        let receipt = window_error(super::super::super::super::uia::WindowError::Unconfirmed {
            operation: "move",
            observed: None,
        });
        assert_eq!(receipt["code"], "window_effect_unconfirmed");
        assert_eq!(receipt["effect_may_have_occurred"], true);
        assert!(
            receipt["error"]
                .as_str()
                .unwrap()
                .contains("Re-observe before retrying")
        );
    }

    #[test]
    fn cancelled_window_action_is_not_admitted() {
        let cancel = AtomicBool::new(false);
        executor::request_job_cancel(&cancel);
        let result = window_admission(&cancel, || panic!("cancelled action was admitted"));
        assert_eq!(result["code"], "cancelled");
        assert_eq!(result["effect_may_have_occurred"], false);
    }
}
