//! Centralized design tokens for the egui settings UI.
//!
//! Build an `AppTheme` once per render (from the active visuals) and read
//! semantic colors / frames from it, instead of hand-branching
//! `if ui.visuals().dark_mode { rgb_a } else { rgb_b }` at every call site.
//! Each token resolves dark vs light internally, so the values live in exactly
//! one place. Only patterns that actually repeat across the UI are tokenized.

use eframe::egui::{Color32, Stroke, Ui};

/// Resolved palette for the current frame. Cheap to construct.
#[derive(Clone, Copy)]
pub struct AppTheme {
    dark: bool,
}

impl AppTheme {
    pub fn from_ui(ui: &Ui) -> Self {
        Self {
            dark: ui.visuals().dark_mode,
        }
    }

    pub fn from_dark(dark: bool) -> Self {
        Self { dark }
    }

    #[inline]
    fn pick(&self, dark: Color32, light: Color32) -> Color32 {
        if self.dark { dark } else { light }
    }

    // --- Surfaces -----------------------------------------------------------

    /// Elevated card / panel face (forms, history rows, preset header card).
    pub fn card_bg(&self) -> Color32 {
        self.pick(
            Color32::from_rgba_unmultiplied(28, 32, 42, 250),
            Color32::from_rgba_unmultiplied(255, 255, 255, 255),
        )
    }

    /// Hairline border around cards.
    pub fn card_stroke(&self) -> Stroke {
        Stroke::new(
            1.0,
            self.pick(Color32::from_gray(50), Color32::from_gray(210)),
        )
    }

    /// Title bar + footer chrome bars.
    pub fn bar_bg(&self) -> Color32 {
        self.pick(Color32::from_gray(20), Color32::from_gray(240))
    }

    // --- Modality accents (sidebar preset chips) ----------------------------

    pub fn modality_image(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(45, 85, 140),
            Color32::from_rgb(100, 150, 220),
        )
    }

    pub fn modality_text(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(45, 120, 80),
            Color32::from_rgb(90, 180, 120),
        )
    }

    pub fn modality_audio(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(150, 95, 40),
            Color32::from_rgb(220, 160, 80),
        )
    }
}
