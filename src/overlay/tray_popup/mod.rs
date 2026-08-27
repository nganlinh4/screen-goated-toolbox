//! Native egui tray popup.
//!
//! The off-screen child viewport stays on the application's existing event loop.
//! Tray interaction reveals and repaints only that child, never the settings root.

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
static PAINTED_GENERATION: AtomicU64 = AtomicU64::new(0);
static PREPAINT_PASSES: AtomicU64 = AtomicU64::new(0);
static SURFACE_READY: AtomicBool = AtomicBool::new(false);

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("sgt-native-tray-popup")
}

fn viewport_title() -> String {
    format!("{VIEWPORT_TITLE} [{}]", std::process::id())
}

/// Toggle the popup at the cursor position captured by the tray event thread.
pub fn show_tray_popup() {
    let started = Instant::now();
    if VISIBLE.swap(true, Ordering::SeqCst) {
        hide_with_reason("tray-toggle");
        return;
    }

    let anchor = win32::cursor_position();
    ANCHOR_X.store(anchor.x, Ordering::SeqCst);
    ANCHOR_Y.store(anchor.y, Ordering::SeqCst);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    crate::log_info!(
        "[TrayPopup] show requested generation={} pid={} anchor=({}, {}) renderer=shared-child",
        generation,
        std::process::id(),
        anchor.x,
        anchor.y
    );
    if SURFACE_READY.load(Ordering::SeqCst)
        && let Some(window) = win32::popup_window()
    {
        let placement = current_placement(current_zoom_factor());
        win32::reveal(window, placement);
        request_popup_repaint();
        crate::log_info!(
            "[TrayPopup] revealed generation={} hwnd={:#x} elapsed_ms={} position=({}, {}) size={}x{} source=offscreen-surface",
            generation,
            window.0 as usize,
            started.elapsed().as_millis(),
            placement.physical_position.x,
            placement.physical_position.y,
            placement.physical_size[0],
            placement.physical_size[1]
        );
    } else {
        std::thread::spawn(move || reveal_when_ready(generation));
    }
    std::thread::spawn(move || monitor_focus(generation));
}

fn reveal_when_ready(generation: u64) {
    let started = Instant::now();
    loop {
        if !is_current(generation) {
            return;
        }
        if let Some(window) = win32::popup_window() {
            let placement = current_placement(current_zoom_factor());
            win32::prepare_offscreen(window, placement);
            request_popup_repaint();

            while is_current(generation)
                && PAINTED_GENERATION.load(Ordering::SeqCst) < generation
                && started.elapsed() < Duration::from_millis(1500)
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            if !is_current(generation) {
                return;
            }
            if PAINTED_GENERATION.load(Ordering::SeqCst) < generation {
                hide_with_reason("paint-timeout");
                return;
            }

            // Give the first submitted frame one compositor turn. This path is
            // reached only when a click races application startup.
            std::thread::sleep(Duration::from_millis(20));
            if !is_current(generation) {
                return;
            }
            win32::reveal(window, placement);
            request_popup_repaint();
            crate::log_info!(
                "[TrayPopup] revealed generation={} hwnd={:#x} elapsed_ms={} position=({}, {}) size={}x{}",
                generation,
                window.0 as usize,
                started.elapsed().as_millis(),
                placement.physical_position.x,
                placement.physical_position.y,
                placement.physical_size[0],
                placement.physical_size[1],
            );
            return;
        }
        if started.elapsed() >= Duration::from_millis(1500) {
            VISIBLE.store(false, Ordering::SeqCst);
            crate::log_info!(
                "[TrayPopup] reveal failed generation={} reason=child-window-unavailable elapsed_ms={}",
                generation,
                started.elapsed().as_millis()
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn monitor_focus(generation: u64) {
    let mut saw_foreground = false;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if !is_current(generation) {
            return;
        }
        let Some(window) = win32::popup_window() else {
            continue;
        };
        if win32::owns_foreground(window) {
            saw_foreground = true;
        } else if saw_foreground {
            let foreground = win32::foreground_identity();
            crate::log_info!(
                "[TrayPopup] focus lost generation={} foreground_hwnd={:#x} foreground_pid={}",
                generation,
                foreground.hwnd,
                foreground.pid
            );
            hide_with_reason("focus-lost");
            return;
        }
    }
}

fn is_current(generation: u64) -> bool {
    VISIBLE.load(Ordering::SeqCst) && GENERATION.load(Ordering::SeqCst) == generation
}

fn hide_with_reason(reason: &'static str) {
    let generation = GENERATION.load(Ordering::SeqCst);
    VISIBLE.store(false, Ordering::SeqCst);
    if let Some(window) = win32::popup_window() {
        win32::hide(window);
    }
    request_popup_repaint();
    crate::log_info!(
        "[TrayPopup] hidden generation={} reason={}",
        generation,
        reason
    );
}

fn request_popup_repaint() -> bool {
    let Ok(context) = crate::gui::GUI_CONTEXT.lock() else {
        return false;
    };
    let Some(context) = context.as_ref() else {
        return false;
    };
    context.request_repaint_of(viewport_id());
    true
}

fn current_zoom_factor() -> f32 {
    crate::gui::GUI_CONTEXT
        .lock()
        .ok()
        .and_then(|context| context.as_ref().map(egui::Context::zoom_factor))
        .unwrap_or(1.0)
}

/// Register the off-screen child whenever a normal settings UI frame is available.
pub fn render(context: &egui::Context) {
    let placement = current_placement(context.zoom_factor());
    context.show_viewport_deferred(viewport_id(), viewport_builder(placement), |ui, _class| {
        render_viewport(ui);
    });
}

fn viewport_builder(placement: layout::PopupPlacement) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(viewport_title())
        .with_position(egui::pos2(-32_000.0, -32_000.0))
        .with_inner_size(placement.size_points)
        .with_min_inner_size(placement.size_points)
        .with_max_inner_size(placement.size_points)
        .with_resizable(false)
        // egui-winit maps `decorations(false)` to winit's Windows-only
        // undecorated-shadow mode. Its WM_NCCALCSIZE handler deliberately
        // reserves one top pixel. Declare decorations here to suppress that
        // mode; `configure_borderless_popup` strips the real HWND frame before
        // this off-screen surface can be revealed.
        .with_decorations(true)
        .with_transparent(false)
        .with_taskbar(false)
        .with_close_button(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_always_on_top()
        // Keep a painted child off-screen. Win32 moves the retained surface;
        // changing these creation properties would recreate its HWND.
        .with_active(false)
        .with_visible(true)
}

fn render_viewport(ui: &mut egui::Ui) {
    let placement = current_placement(ui.ctx().zoom_factor());
    if !VISIBLE.load(Ordering::SeqCst) {
        if let Some(window) = win32::popup_window() {
            win32::prepare_offscreen(window, placement);
        }
        ui::prepaint(ui, placement);
        let pass = PREPAINT_PASSES.fetch_add(1, Ordering::SeqCst) + 1;
        if pass >= 2 {
            if !SURFACE_READY.swap(true, Ordering::SeqCst) {
                let hwnd = win32::popup_window()
                    .map(|window| window.0 as usize)
                    .unwrap_or_default();
                crate::log_info!(
                    "[TrayPopup] offscreen surface ready hwnd={hwnd:#x} prepaint_passes={pass}"
                );
            }
        } else {
            ui.ctx().request_repaint_of(viewport_id());
        }
        return;
    }
    let generation = GENERATION.load(Ordering::SeqCst);
    let first_paint = ui::begin_generation(generation);
    ui::render(ui, placement, generation);
    PAINTED_GENERATION.store(generation, Ordering::SeqCst);
    if first_paint {
        crate::log_info!("[TrayPopup] first paint generation={generation}");
    }
}

fn current_placement(zoom_factor: f32) -> layout::PopupPlacement {
    let anchor = layout::PhysicalPoint {
        x: ANCHOR_X.load(Ordering::SeqCst),
        y: ANCHOR_Y.load(Ordering::SeqCst),
    };
    let monitor = win32::monitor_metrics(anchor, zoom_factor);
    layout::place(
        anchor,
        monitor.work_area,
        monitor.pixels_per_point,
        data::restore_option_count(),
    )
}

/// Shared egui visuals are owned by the main context; repaint only the child.
pub fn update_theme(_is_dark: bool) {
    if VISIBLE.load(Ordering::SeqCst) {
        request_popup_repaint();
    }
}

pub(super) fn close_from_viewport(context: &egui::Context, reason: &'static str) {
    if context.input(|input| input.viewport().close_requested()) {
        context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    }
    hide_with_reason(reason);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_creation_properties_avoid_winit_shadow_mode() {
        let placement = layout::place(
            layout::PhysicalPoint { x: 100, y: 100 },
            layout::WorkArea {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            1.0,
            0,
        );
        let builder = viewport_builder(placement);
        assert_eq!(builder.active, Some(false));
        assert_eq!(builder.visible, Some(true));
        assert_eq!(builder.decorations, Some(true));
        assert_eq!(builder.position, Some(egui::pos2(-32_000.0, -32_000.0)));
    }
}
