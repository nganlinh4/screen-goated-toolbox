// --- ENHANCED ICON PAINTER MODULE V2 ---
// High-fidelity programmatic vector icons for egui.
// No assets, no fonts, pure math.

mod paint;

use eframe::egui;

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Settings,

    EyeOpen,
    EyeClosed,
    Microphone,
    Image,

    Text,        // NEW: 'T' icon for text presets
    Delete,      // Renders as Trash Can (used for presets)
    DeleteLarge, // NEW: Centered, larger Trash Can (used for history items)

    Folder,    // NEW: For "Open Media"
    Copy,      // NEW: For "Copy Text"
    CopySmall, // NEW: Smaller copy icon for preset buttons
    Close,     // NEW: "X" for clearing search

    TextSelect,      // NEW: Text with selection cursor for text selection mode
    Speaker,         // NEW: Speaker icon for device audio source
    SpeakerDisabled, // NEW: Speaker with cross (disabled TTS)
    CopyDisabled,    // NEW: Copy icon with cross (disabled auto-copy)
    Lightbulb,       // NEW: Lightbulb icon for tips
    Realtime,        // NEW: Streaming waves icon for realtime audio processing
    Star,            // Outline star for non-favorite presets
    StarFilled,      // Filled star for favorite presets
    Sun,             // New: Sun icon for light mode
    Moon,            // New: Moon icon for dark mode
    Device,          // New: Monitor/Device icon for system theme
    DragHandle,      // New: Drag handle for reordering
    History,         // New: History icon (clock)
    Parakeet,        // New: Parakeet icon (Bird)
    Pointer,         // New: Mouse pointer/cursor icon

    // Window Controls
    Minimize,
    Maximize,
    Restore,
}

/// Main entry point: Draw a clickable icon button (default size 24.0)
pub fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    icon_button_sized(ui, icon, 24.0)
}

/// Draw a clickable icon button with custom size
pub fn icon_button_sized(ui: &mut egui::Ui, icon: Icon, size_val: f32) -> egui::Response {
    let size = egui::vec2(size_val, size_val);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    // 1. Background Hover Effect
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), 4.0, ui.visuals().widgets.hovered.bg_fill);
    }

    // 2. Determine Style
    let color = if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };

    // 3. Paint
    paint::paint_internal(ui.painter(), rect, icon, color);

    response
}

/// Draw a static icon (for labels/headers)
pub fn draw_icon_static(ui: &mut egui::Ui, icon: Icon, size_override: Option<f32>) {
    let side = size_override.unwrap_or(16.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let color = ui.visuals().text_color();
    paint::paint_internal(ui.painter(), rect, icon, color);
}

/// Public function to paint an icon directly (for custom layouts where icon_button isn't suitable)
pub fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    paint::paint_internal(painter, rect, icon, color);
}
