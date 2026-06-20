//! Computer Control overlay: an always-on-top egui child viewport showing the
//! live session status, what the user said, what the model is doing (action
//! log), and a STOP button. Also owns the session lifecycle (start/stop the
//! background runtime thread). Rendered each frame from the main egui app.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use eframe::egui;

static CC_ACTIVE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));
static CC_STOP: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));
static CC_STATE: LazyLock<Arc<Mutex<ComputerControlState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(ComputerControlState::default())));

/// Shared, UI-facing state of the running session.
pub(super) struct ComputerControlState {
    pub status: String,
    pub listening: bool,
    pub user_text: String,
    pub model_text: String,
    pub log: Vec<String>,
}

impl Default for ComputerControlState {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            listening: false,
            user_text: String::new(),
            model_text: String::new(),
            log: Vec::new(),
        }
    }
}

pub fn is_active() -> bool {
    CC_ACTIVE.load(Ordering::SeqCst)
}

/// Start a Computer Control session (no-op if one is already running).
pub fn show_overlay() {
    if CC_ACTIVE.swap(true, Ordering::SeqCst) {
        return; // already active
    }
    CC_STOP.store(false, Ordering::SeqCst);
    {
        let mut s = CC_STATE.lock().unwrap();
        *s = ComputerControlState::default();
        s.status = "connecting…".to_string();
    }
    let stop = CC_STOP.clone();
    std::thread::spawn(move || {
        super::runtime::run(stop);
        CC_ACTIVE.store(false, Ordering::SeqCst);
        request_repaint();
    });
    request_repaint();
}

/// Signal the running session to stop (the runtime thread clears CC_ACTIVE).
pub fn stop_overlay() {
    CC_STOP.store(true, Ordering::SeqCst);
    request_repaint();
}

// --- runtime-facing state helpers ---

pub(super) fn set_status(status: impl Into<String>) {
    let status = status.into();
    eprintln!("[cc] status: {status}");
    if let Ok(mut s) = CC_STATE.lock() {
        s.status = status;
    }
}

pub(super) fn set_listening(on: bool) {
    if let Ok(mut s) = CC_STATE.lock() {
        s.listening = on;
    }
}

pub(super) fn set_user_text(text: impl Into<String>) {
    let text = text.into();
    eprintln!("[cc] you: {text}");
    if let Ok(mut s) = CC_STATE.lock() {
        s.user_text = text;
    }
}

pub(super) fn set_model_text(text: impl Into<String>) {
    let text = text.into();
    if let Ok(mut s) = CC_STATE.lock() {
        s.model_text = text;
    }
}

pub(super) fn push_log(line: impl Into<String>) {
    let line = line.into();
    eprintln!("[cc] {line}");
    if let Ok(mut s) = CC_STATE.lock() {
        s.log.push(line);
        let len = s.log.len();
        if len > 200 {
            s.log.drain(0..len - 200);
        }
    }
    request_repaint();
}

fn request_repaint() {
    if let Ok(guard) = crate::gui::GUI_CONTEXT.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

/// Render the overlay viewport. Call once per frame from the main egui app.
pub fn render_overlay(ctx: &egui::Context) {
    if !CC_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("computer_control_overlay"),
        egui::ViewportBuilder::default()
            .with_inner_size([520.0, 440.0])
            .with_title("Điều khiển máy tính")
            .with_always_on_top(),
        |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                stop_overlay();
            }
            let is_dark = ctx.style().visuals.dark_mode;
            let panel_fill = if is_dark {
                egui::Color32::from_rgba_premultiplied(24, 24, 28, 244)
            } else {
                egui::Color32::from_rgba_premultiplied(252, 250, 255, 244)
            };
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(panel_fill)
                        .inner_margin(egui::Margin::same(12)),
                )
                .show_inside(ctx, |ui| render_body(ui, is_dark));
        },
    );
}

fn render_body(ui: &mut egui::Ui, is_dark: bool) {
    let state = CC_STATE.lock().unwrap();
    let accent = egui::Color32::from_rgb(0, 200, 255);
    let muted = if is_dark {
        egui::Color32::from_gray(170)
    } else {
        egui::Color32::from_gray(90)
    };

    ui.horizontal(|ui| {
        let dot = if state.listening {
            egui::Color32::from_rgb(80, 220, 120)
        } else {
            accent
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, dot);
        ui.heading(if state.listening { "Listening…" } else { state.status.as_str() });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(egui::RichText::new("■ STOP").strong()).fill(
                    egui::Color32::from_rgb(200, 60, 60),
                ))
                .clicked()
            {
                stop_overlay();
            }
        });
    });
    ui.add_space(6.0);

    if !state.user_text.is_empty() {
        ui.label(egui::RichText::new(format!("You: {}", state.user_text)).color(muted));
    }
    if !state.model_text.is_empty() {
        ui.label(egui::RichText::new(&state.model_text).color(accent));
    }
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &state.log {
                ui.label(egui::RichText::new(line).monospace().size(12.0));
            }
        });
}
