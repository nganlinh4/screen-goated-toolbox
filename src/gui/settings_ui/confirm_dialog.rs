//! Reusable Material-style confirmation dialog.
//!
//! Wraps `egui::Modal` with the shared [`AppTheme`] dialog tokens so every
//! yes/no confirmation in the app gets one clean, consistent look. Callers keep
//! their own "is this dialog open?" state and react to the returned
//! [`ConfirmResult`] each frame.

use crate::gui::theme::AppTheme;
use eframe::egui;

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
    confirm_enabled: bool,
    warning: Option<&'a str>,
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
            confirm_enabled: true,
            warning: None,
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

    /// Disable confirmation while a conflicting operation is active.
    pub fn confirm_enabled(mut self, enabled: bool) -> Self {
        self.confirm_enabled = enabled;
        self
    }

    /// Optional caution text rendered using the shared semantic warning color.
    pub fn warning(mut self, warning: Option<&'a str>) -> Self {
        self.warning = warning;
        self
    }

    pub fn show(self, ui: &mut egui::Ui, theme: &AppTheme) -> ConfirmResult {
        self.show_ctx(ui.ctx(), theme)
    }

    pub fn show_ctx(self, ctx: &egui::Context, theme: &AppTheme) -> ConfirmResult {
        let mut result = ConfirmResult::Pending;

        let modal = crate::gui::widgets::material_modal(ctx, theme, self.id, |ui| {
            ui.set_width(290.0);

            crate::gui::widgets::dialog_title(ui, theme, self.title);

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

            crate::gui::widgets::dialog_body(ui, theme, self.body);

            if let Some(warning) = self.warning {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(warning)
                        .size(12.0)
                        .color(theme.warning()),
                );
            }

            ui.add_space(18.0);

            // Actions right-aligned, affirmative button on the right (M3).
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                let confirm_fill = if self.destructive {
                    theme.danger_fill()
                } else {
                    theme.accent_fill()
                };
                let confirm = ui
                    .add_enabled_ui(self.confirm_enabled, |ui| {
                        crate::gui::widgets::filled_button(
                            ui,
                            self.confirm_label,
                            confirm_fill,
                            theme.on_accent(),
                            16,
                        )
                    })
                    .inner;
                if confirm.clicked() {
                    result = ConfirmResult::Confirmed;
                }
                if crate::gui::widgets::filled_button(
                    ui,
                    self.cancel_label,
                    theme.neutral_fill(),
                    theme.on_surface(),
                    16,
                )
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
