use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::component_registry::recovery::{self, RecoveryEntry};

#[derive(Default)]
struct State {
    loaded: bool,
    busy: bool,
    entries: Vec<RecoveryEntry>,
    recorder: Vec<(PathBuf, String)>,
    message: Option<String>,
    last_refresh: Option<Instant>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

pub(super) fn render(ui: &mut egui::Ui) {
    let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
    if !state.busy
        && (!state.loaded
            || state
                .last_refresh
                .is_none_or(|time| time.elapsed() >= Duration::from_secs(5)))
    {
        state.busy = true;
        refresh(ui.ctx().clone());
    }
    ui.ctx().request_repaint_after(Duration::from_secs(5));
    let mut selected = None;
    if !state.entries.is_empty() || !state.recorder.is_empty() || state.message.is_some() {
        ui.collapsing("Preserved component files", |ui| {
            if let Some(message) = &state.message {
                ui.label(message);
            }
            for entry in &state.entries {
                ui.separator();
                ui.label(entry.path.display().to_string());
                ui.label(&entry.reason);
                if entry.can_clean
                    && ui
                        .add_enabled(!state.busy, egui::Button::new("Clean unchanged files"))
                        .clicked()
                {
                    let entry = entry.clone();
                    selected = Some(entry);
                    break;
                }
            }
            for (path, reason) in &state.recorder {
                ui.separator();
                ui.label(path.display().to_string());
                ui.label(reason);
            }
        });
        ui.add_space(8.0);
    }
    if let Some(entry) = selected {
        state.busy = true;
        clean(entry, ui.ctx().clone());
    }
}

fn refresh(ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = recovery::list();
        let recorder = crate::component_registry::recorder::recoveries();
        let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
        state.loaded = true;
        state.busy = false;
        state.last_refresh = Some(Instant::now());
        match result {
            Ok(entries) => state.entries = entries,
            Err(error) => {
                state.message = Some(format!("Could not list preserved files: {error:#}"))
            }
        }
        match recorder {
            Ok(entries) => state.recorder = entries,
            Err(error) => {
                state.message = Some(format!("Could not list recorder recovery files: {error:#}"))
            }
        }
        ctx.request_repaint();
    });
}

fn clean(entry: RecoveryEntry, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = recovery::clean(&entry);
        let entries = recovery::list();
        let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
        state.busy = false;
        state.message = match result {
            Ok(outcome) if outcome.preserved_paths.is_empty() => None,
            Ok(outcome) => Some(format!(
                "Preserved {} modified or unknown path(s) at {}.",
                outcome.preserved_paths.len(),
                outcome.path.display()
            )),
            Err(error) => Some(format!("Could not clean preserved files: {error:#}")),
        };
        if let Ok(entries) = entries {
            state.entries = entries;
        }
        ctx.request_repaint();
    });
}
