//! Reusable Material-style confirmation dialog.
//!
//! Wraps `egui::Modal` with the shared [`AppTheme`] dialog tokens so every
//! yes/no confirmation in the app gets one clean, consistent look. Callers keep
//! their own "is this dialog open?" state and react to the returned
//! [`ConfirmResult`] each frame.

use crate::gui::theme::{blend, AppTheme};
use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Outcome of a single frame of a confirmation dialog.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConfirmResult {
    /// No button pressed yet — keep showing the dialog.
    Pending,
    /// The user accepted the (possibly destructive) action.
    Confirmed,
    /// The user dismissed via Cancel, the backdrop, or Escape.
    Cancelled,
}

/// A centered, scrimmed confirmation dialog rendered with the shared dialog
/// styling. Build it, then call [`ConfirmModal::show`] every frame the dialog
/// should be visible.
pub struct ConfirmModal<'a> {
    id: egui::Id,
    title: &'a str,
    emphasis: Option<&'a str>,
    body: &'a str,
    confirm_label: &'a str,
    cancel_label: &'a str,
    destructive: bool,
}

impl<'a> ConfirmModal<'a> {
    pub fn new(id: egui::Id, title: &'a str, body: &'a str) -> Self {
        Self {
            id,
            title,
            emphasis: None,
            body,
            confirm_label: "OK",
            cancel_label: "Cancel",
            destructive: false,
        }
    }

    /// A short, strong line shown above the body (e.g. the name of the item
    /// being acted on).
    pub fn emphasis(mut self, text: &'a str) -> Self {
        self.emphasis = Some(text);
        self
    }

    /// Override the action button labels (localized by the caller).
    pub fn labels(mut self, confirm: &'a str, cancel: &'a str) -> Self {
        self.confirm_label = confirm;
        self.cancel_label = cancel;
        self
    }

    /// Render the confirm action with destructive (red) emphasis.
    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    pub fn show(self, ui: &mut egui::Ui, theme: &AppTheme) -> ConfirmResult {
        let mut result = ConfirmResult::Pending;

        let modal = egui::Modal::new(self.id)
            .backdrop_color(theme.scrim_color())
            .frame(theme.dialog_frame())
            .show(ui.ctx(), |ui| {
                ui.set_width(290.0);

                ui.label(
                    egui::RichText::new(self.title)
                        .size(16.5)
                        .strong()
                        .color(theme.on_surface()),
                );

                if let Some(emphasis) = self.emphasis {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(emphasis)
                            .size(14.0)
                            .strong()
                            .color(theme.on_surface()),
                    );
                    ui.add_space(3.0);
                } else {
                    ui.add_space(7.0);
                }

                ui.label(
                    egui::RichText::new(self.body)
                        .size(12.5)
                        .color(theme.on_surface_variant()),
                );

                ui.add_space(18.0);

                // Actions right-aligned, affirmative button on the right (M3).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    let confirm_fill = if self.destructive {
                        theme.danger_fill()
                    } else {
                        theme.accent_fill()
                    };
                    if pill_button(ui, self.confirm_label, confirm_fill, theme.on_accent()).clicked()
                    {
                        result = ConfirmResult::Confirmed;
                    }
                    if pill_button(ui, self.cancel_label, theme.neutral_fill(), theme.on_surface())
                        .clicked()
                    {
                        result = ConfirmResult::Cancelled;
                    }
                });
            });

        if result == ConfirmResult::Pending && modal.should_close() {
            result = ConfirmResult::Cancelled;
        }

        result
    }
}

/// A compact, fully-rounded button with Material-style hover/press state layers
/// (the `text` color overlaid at 8% / 14% over `fill`), so it reads correctly
/// in both light and dark themes.
fn pill_button(
    ui: &mut egui::Ui,
    label: &str,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(16.0, 8.0);
        let widgets = &mut ui.visuals_mut().widgets;
        for (visual, state_fill) in [
            (&mut widgets.inactive, fill),
            (&mut widgets.hovered, blend(fill, text, 0.08)),
            (&mut widgets.active, blend(fill, text, 0.14)),
        ] {
            visual.weak_bg_fill = state_fill;
            visual.bg_fill = state_fill;
            visual.bg_stroke = Stroke::NONE;
        }
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(12.5).color(text))
                .corner_radius(CornerRadius::same(16))
                .min_size(egui::vec2(0.0, 30.0)),
        )
    })
    .inner
}
