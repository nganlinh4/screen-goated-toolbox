use crate::gui::icons::{self, Icon};
use crate::gui::theme::{AppTheme, blend};
use eframe::egui::{self, Color32};

/// Representative accent for a provider — drives action buttons and row icons
/// so each provider reads at a glance.
///
/// Dark variants are lighter/brighter (legible on dark surfaces); light variants
/// are deeper/more saturated (legible as text on near-white surfaces). Used as a
/// solid color for icons/text/buttons; for fills, blend it into the surface with
/// [`wash`] so transparency never depends on what's painted underneath.
pub(super) fn provider_accent(provider: &str, dark: bool) -> Color32 {
    let (d, l) = match provider {
        "google" => ((124, 156, 245), (66, 92, 210)),
        "groq" => ((236, 154, 74), (176, 92, 18)),
        "cerebras" => ((230, 116, 100), (192, 58, 42)),
        "openrouter" => ((112, 152, 236), (52, 96, 200)),
        "ollama" => ((96, 198, 152), (28, 140, 92)),
        _ => ((124, 154, 204), (64, 96, 168)),
    };
    let (r, g, b) = if dark { d } else { l };
    Color32::from_rgb(r, g, b)
}

/// Blend [color] into the card surface by fraction [t] to get a faint, *opaque*
/// tint. Unlike a low-alpha overlay, this reads identically regardless of what's
/// behind it, and adapts to dark/light because it starts from `card_bg`.
pub(super) fn wash(theme: &AppTheme, color: Color32, t: f32) -> Color32 {
    blend(theme.card_bg(), color, t)
}

/// A legible on-color (near-black or white) for text/icons painted on top of a
/// solid [fill]. Picks by perceived luminance so it works for both the lighter
/// dark-mode accents and the deeper light-mode ones.
pub(super) fn on_color(fill: Color32) -> Color32 {
    let luma = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luma > 150.0 {
        Color32::from_rgb(20, 22, 28)
    } else {
        Color32::WHITE
    }
}

/// Paint a provider/status icon at [size], tinted [color], inline in the layout.
pub(super) fn accent_icon(ui: &mut egui::Ui, icon: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    icons::paint_icon(ui.painter(), rect, icon, color);
}
