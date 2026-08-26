//! Native egui tray popup.
//!
//! The popup is a deferred viewport owned by the already-running settings renderer,
//! so the first tray click never has to initialize another browser process.

mod data;
mod layout;
mod ui;
mod win32;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use eframe::egui;

const VIEWPORT_TITLE: &str = "SGT Tray Quick Actions";

static VISIBLE: AtomicBool = AtomicBool::new(false);
static ANCHOR_X: AtomicI32 = AtomicI32::new(0);
static ANCHOR_Y: AtomicI32 = AtomicI32::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("sgt-native-tray-popup")
}

fn request_gui_repaint() {
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// Toggle the popup at the cursor position captured by the tray event thread.
pub fn show_tray_popup() {
    if VISIBLE.load(Ordering::SeqCst) {
        hide_tray_popup();
        return;
    }

    let anchor = win32::cursor_position();
    crate::log_info!(
        "[TrayPopup] native show requested anchor=({}, {})",
        anchor.x,
        anchor.y
    );
    ANCHOR_X.store(anchor.x, Ordering::SeqCst);
    ANCHOR_Y.store(anchor.y, Ordering::SeqCst);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    VISIBLE.store(true, Ordering::SeqCst);
    request_gui_repaint();
    std::thread::spawn(move || monitor_focus(generation));
}

fn monitor_focus(generation: u64) {
    let grace_until = Instant::now() + Duration::from_millis(650);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if !VISIBLE.load(Ordering::SeqCst) || GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if Instant::now() < grace_until {
            continue;
        }
        let Some(window) = win32::popup_window() else {
            continue;
        };
        if !win32::owns_foreground(window) {
            hide_tray_popup();
            return;
        }
    }
}

/// Close the native viewport. A later tray click creates it again without a WebView.
pub fn hide_tray_popup() {
    crate::log_info!("[TrayPopup] native hide requested");
    VISIBLE.store(false, Ordering::SeqCst);
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
        context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// Shared egui visuals update automatically; repaint an open popup immediately.
pub fn update_theme(_is_dark: bool) {
    if VISIBLE.load(Ordering::SeqCst) {
        request_gui_repaint();
    }
}

/// Register the child viewport from every root frame while the popup is open.
pub fn render(context: &egui::Context) {
    if !VISIBLE.load(Ordering::SeqCst) {
        return;
    }

    let anchor = layout::PhysicalPoint {
        x: ANCHOR_X.load(Ordering::SeqCst),
        y: ANCHOR_Y.load(Ordering::SeqCst),
    };
    let generation = GENERATION.load(Ordering::SeqCst);
    let option_count = data::restore_option_count();
    let monitor = win32::monitor_metrics(anchor, context.zoom_factor());
    let placement = layout::place(
        anchor,
        monitor.work_area,
        monitor.pixels_per_point,
        option_count,
    );
    if ui::begin_generation(generation) {
        crate::log_info!(
            "[TrayPopup] native viewport requested generation={} options={}",
            generation,
            option_count
        );
    }

    let builder = egui::ViewportBuilder::default()
        .with_title(VIEWPORT_TITLE)
        .with_position(placement.position_points)
        .with_inner_size(placement.size_points)
        .with_min_inner_size(placement.size_points)
        .with_max_inner_size(placement.size_points)
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(false)
        .with_taskbar(false)
        .with_close_button(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_always_on_top()
        .with_active(true);

    context.show_viewport_deferred(viewport_id(), builder, move |ui, _class| {
        ui::render(ui, placement, generation);
    });
    context.request_repaint_of(viewport_id());
}

pub(super) fn close_from_viewport(context: &egui::Context) {
    VISIBLE.store(false, Ordering::SeqCst);
    context.send_viewport_cmd(egui::ViewportCommand::Close);
    request_gui_repaint();
}
