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
            "system_query" => super::super::super::system_query::query(args),
            "list_files" => super::super::super::system_query::list_files(args),
            "read_text_file" => executor::execute_ex(name, args, &self.profile, cancel),
            "scroll" => self.dispatch_scroll(args, cancel),
            "drag" => self.dispatch_grid_drag(args, cancel),
            "focus_window" => self.dispatch_focus_window(args),
            "list_windows" => {
                json!({"ok": true, "windows": super::super::super::uia::list_windows()})
            }
            "read_clipboard" => {
                json!({"ok": true, "text": super::super::super::clipboard::get_text()})
            }
            "minimize_window" => self.dispatch_minimize_window(args),
            "resize_window" => self.dispatch_resize_window(args),
            "move_window" => self.dispatch_move_window(args),
            _ => return None,
        })
    }

    fn dispatch_scroll(&self, args: &Value, cancel: &Arc<AtomicBool>) -> Value {
        let (mx, my) = args
            .get("cell")
            .and_then(Value::as_u64)
            .and_then(|cell| self.grid.center_norm(cell as u32))
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
        let from = args
            .get("from_cell")
            .and_then(Value::as_u64)
            .and_then(|cell| self.grid.center_norm(cell as u32));
        let to = args
            .get("to_cell")
            .and_then(Value::as_u64)
            .and_then(|cell| self.grid.center_norm(cell as u32));
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

    fn dispatch_minimize_window(&self, args: &Value) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        match super::super::super::uia::minimize_window(title) {
            Err(error) => window_error(error),
            Ok(ok) => {
                std::thread::sleep(Duration::from_millis(200));
                json!({"ok": ok, "foreground_now": super::super::super::uia::pointer_context().0})
            }
        }
    }

    fn dispatch_resize_window(&self, args: &Value) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        let width = args.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
        let height = args.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;
        super::super::super::uia::resize_window(title, width, height)
            .map(|ok| json!({"ok": ok}))
            .unwrap_or_else(window_error)
    }

    fn dispatch_move_window(&self, args: &Value) -> Value {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        let x = args.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
        let y = args.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
        super::super::super::uia::move_window(title, x, y)
            .map(|ok| json!({"ok": ok}))
            .unwrap_or_else(window_error)
    }
}

fn window_error(error: super::super::super::uia::WindowError) -> Value {
    json!({
        "ok": false,
        "code": error.code(),
        "error": error.to_string(),
    })
}
