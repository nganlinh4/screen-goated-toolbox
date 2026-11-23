use std::sync::Mutex;

#[derive(Default)]
struct State {
    frame: bool,
    ready: bool,
    conflict: bool,
    exiting: bool,
    file_activation: bool,
    startup_operations: usize,
}

pub(super) struct Gate(Mutex<State>);

impl Gate {
    const fn new() -> Self {
        Self(Mutex::new(State {
            frame: false,
            ready: false,
            conflict: false,
            exiting: false,
            file_activation: false,
            startup_operations: 0,
        }))
    }

    fn frame(&self) -> Option<Frame<'_>> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.exiting || state.frame {
            return None;
        }
        state.frame = true;
        Some(Frame {
            gate: self,
            finished: false,
        })
    }

    fn exit(&self) -> Result<Exit<'_>, Denied> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.conflict {
            return Err(Denied::Conflict);
        }
        if !state.ready || state.frame || state.exiting || state.startup_operations != 0 {
            return Err(Denied::Busy);
        }
        state.exiting = true;
        Ok(Exit(self))
    }

    fn startup(&self) -> Option<StartupPermit<'_>> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.exiting {
            return None;
        }
        state.startup_operations = state.startup_operations.checked_add(1)?;
        Some(StartupPermit(self))
    }

    fn mark_file_activation(&self) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .file_activation = true;
    }

    fn automatic_restart_blocked(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .file_activation
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Denied {
    Busy,
    Conflict,
}

pub(super) struct Frame<'a> {
    gate: &'a Gate,
    finished: bool,
}
impl Frame<'_> {
    pub(super) fn finish(mut self, conflict: bool, ready: bool) {
        let mut state = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
        state.conflict = conflict;
        state.ready = ready;
        state.frame = false;
        self.finished = true;
    }
}
impl Drop for Frame<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let mut state = self.gate.0.lock().unwrap_or_else(|e| e.into_inner());
            state.conflict = true;
            state.frame = false;
        }
    }
}

pub(super) struct Exit<'a>(&'a Gate);

pub(crate) struct StartupPermit<'a>(&'a Gate);
impl Drop for StartupPermit<'_> {
    fn drop(&mut self) {
        self.0
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .startup_operations -= 1;
        if let Ok(context) = crate::gui::GUI_CONTEXT.try_lock()
            && let Some(context) = context.as_ref()
        {
            context.request_repaint();
        }
    }
}
impl Drop for Exit<'_> {
    fn drop(&mut self) {
        self.0.0.lock().unwrap_or_else(|e| e.into_inner()).exiting = false;
        if let Ok(context) = crate::gui::GUI_CONTEXT.try_lock()
            && let Some(context) = context.as_ref()
        {
            context.request_repaint();
        }
    }
}

static GATE: Gate = Gate::new();
pub(super) fn begin_frame() -> Option<Frame<'static>> {
    GATE.frame()
}

pub(super) fn cancel_close_during_exit(ctx: &eframe::egui::Context) {
    if ctx.input(|input| input.viewport().close_requested()) {
        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Visible(false));
    }
}
pub(super) fn begin_exit() -> Result<Exit<'static>, Denied> {
    GATE.exit()
}
pub(crate) fn begin_startup_operation() -> Option<std::sync::Arc<StartupPermit<'static>>> {
    GATE.startup().map(std::sync::Arc::new)
}
pub(super) fn mark_file_activation() {
    GATE.mark_file_activation();
}
pub(super) fn automatic_restart_blocked() -> bool {
    GATE.automatic_restart_blocked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_permits_retain_exit_admission_through_worker_and_completion() {
        let gate = Gate::new();
        let frame = gate.frame().unwrap();
        let permit = std::sync::Arc::new(gate.startup().unwrap());
        let worker = permit.clone();
        let completion = permit.clone();
        frame.finish(false, true);
        assert!(matches!(gate.exit(), Err(Denied::Busy)));
        drop(permit);
        drop(worker);
        assert!(matches!(gate.exit(), Err(Denied::Busy)));
        drop(completion);
        let exit = gate.exit().unwrap();
        assert!(gate.startup().is_none());
        drop(exit);
        assert!(gate.startup().is_some());
    }

    #[test]
    fn exits_wait_for_frame_and_resolution() {
        let gate = Gate::new();
        assert!(matches!(gate.exit(), Err(Denied::Busy)));
        let frame = gate.frame().unwrap();
        assert!(matches!(gate.exit(), Err(Denied::Busy)));
        frame.finish(true, true);
        assert!(matches!(gate.exit(), Err(Denied::Conflict)));
        gate.frame().unwrap().finish(false, true);
        let exit = gate.exit().unwrap();
        assert!(gate.frame().is_none());
        drop(exit);
        assert!(gate.frame().is_some());
    }

    #[test]
    fn abandoned_frame_fails_closed_until_next_completed_frame() {
        let gate = Gate::new();
        drop(gate.frame().unwrap());
        assert!(matches!(gate.exit(), Err(Denied::Conflict)));
        gate.frame().unwrap().finish(false, true);
        assert!(gate.exit().is_ok());
    }

    #[test]
    fn blocked_frame_cancels_native_close() {
        use eframe::egui;
        let gate = Gate::new();
        gate.frame().unwrap().finish(false, true);
        let _exit = gate.exit().unwrap();
        assert!(gate.frame().is_none());
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output =
            egui::Context::default().run_ui(input, |ui| cancel_close_during_exit(ui.ctx()));
        let cancelled = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose));
        let hidden = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Visible(false)));
        output.textures_delta.clear();
        assert!(cancelled);
        assert!(hidden);
    }

    #[test]
    fn pending_startup_activation_blocks_exit_until_processed() {
        let gate = Gate::new();
        gate.frame().unwrap().finish(false, false);
        assert!(matches!(gate.exit(), Err(Denied::Busy)));
        gate.frame().unwrap().finish(false, true);
        assert!(gate.exit().is_ok());
    }

    #[test]
    fn startup_file_activation_suppresses_automatic_restart_for_process_lifetime() {
        let gate = Gate::new();
        assert!(!gate.automatic_restart_blocked());
        gate.mark_file_activation();
        gate.frame().unwrap().finish(false, true);
        assert!(gate.automatic_restart_blocked());
        assert!(gate.exit().is_ok());
    }
}
