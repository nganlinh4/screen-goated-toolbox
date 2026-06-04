//! Reusable Material-style widgets shared across the egui settings UI.
//!
//! egui derives a widget's hover/press surface from its `Visuals::widgets`
//! state layers. But an explicit `Button::fill(color)` *overrides* those state
//! layers, so a colored button rendered the naive way stays perfectly flat —
//! it loses all hover/press feedback. These helpers replicate the confirm
//! dialog's `pill_button` trick: temporarily push per-state fills (the resting
//! color plus the `text` color overlaid at 8% / 14%) into the local visuals via
//! `ui.scope`, then add a plain `Button` so egui picks the correct fill for the
//! current interaction state. The result reads correctly in both themes.
//!
//! Module path: `crate::gui::widgets`.

use crate::gui::theme::{blend, AppTheme};
use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Standard Material header for the settings modals.
///
/// Lays out, on one row: a large bold `title`, then any inline `actions`
/// (left-to-right — e.g. restore / clear / size controls / folder), and a close
/// (×) button pinned to the far right. An optional `description` renders below
/// in small muted text, replacing the old separator rule. Returns `true` if the
/// close button was clicked.
pub fn dialog_header(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    title: &str,
    description: Option<&str>,
    actions: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(18.0)
                .strong()
                .color(theme.on_surface()),
        );
        // Description sits inline on the same row, just after the title.
        if let Some(desc) = description {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(desc)
                    .size(11.5)
                    .color(theme.on_surface_variant()),
            );
        }
        ui.add_space(12.0);
        actions(ui);
        // Close pinned to the far right; consumes the remaining row width.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::gui::icons::icon_button(ui, crate::gui::icons::Icon::Close).clicked() {
                close = true;
            }
        });
    });
    ui.add_space(10.0);
    close
}

/// A Material-style filled button that keeps hover/press feedback.
///
/// `fill` is the resting surface, `text` the label/on-color used both for the
/// text and to derive the hover (8%) and pressed (14%) state layers.
/// `corner_radius` sets the rounding in logical pixels.
///
/// Returns the button's [`egui::Response`] so callers can check `.clicked()`,
/// attach tooltips, etc.
pub fn filled_button(
    ui: &mut egui::Ui,
    label: &str,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
) -> egui::Response {
    filled_button_sized(ui, label, fill, text, corner_radius, egui::Vec2::ZERO)
}

/// Like [`filled_button`], but enforces a minimum button size.
///
/// `min_size` is the smallest allowed `(width, height)` in logical pixels; the
/// button still grows to fit its label. Pass [`egui::Vec2::ZERO`] for no
/// minimum (which is exactly what [`filled_button`] does).
pub fn filled_button_sized(
    ui: &mut egui::Ui,
    label: &str,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
    min_size: egui::Vec2,
) -> egui::Response {
    ui.scope(|ui| {
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
            egui::Button::new(egui::RichText::new(label).color(text))
                .corner_radius(CornerRadius::same(corner_radius))
                .min_size(min_size),
        )
    })
    .inner
}

/// A Material-style filled button with a leading icon and a label.
///
/// Lays out a ~16px-square `icon` followed by a small gap and the `label`,
/// inside a filled, `corner_radius`-rounded rect. `fill` is the resting surface;
/// `text` colors both the glyph and the label, and derives the Material state
/// layer overlaid on hover (8%) and press (14%) via [`blend`].
///
/// Returns the button's [`egui::Response`] so callers can check `.clicked()`,
/// attach tooltips, etc.
pub fn filled_icon_button(
    ui: &mut egui::Ui,
    icon: crate::gui::icons::Icon,
    label: &str,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
) -> egui::Response {
    let label_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::TextStyle::Button.resolve(ui.style()),
        text,
    );
    let icon_size = crate::gui::icons::ICON_MD;
    let icon_gap = 6.0;
    let h_pad = ui.spacing().button_padding.x.max(10.0);
    let button_size = egui::vec2(
        h_pad + icon_size + icon_gap + label_galley.rect.width() + h_pad,
        ui.spacing()
            .interact_size
            .y
            .max(label_galley.rect.height() + ui.spacing().button_padding.y * 2.0),
    );

    let (button_rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

    // Material state layer: blend the resting fill toward the on-color.
    let surface = if response.is_pointer_button_down_on() {
        blend(fill, text, 0.14)
    } else if response.hovered() {
        blend(fill, text, 0.08)
    } else {
        fill
    };

    let painter = ui.painter();
    painter.rect_filled(button_rect, CornerRadius::same(corner_radius), surface);

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            button_rect.left() + h_pad,
            button_rect.center().y - icon_size / 2.0,
        ),
        egui::vec2(icon_size, icon_size),
    );
    crate::gui::icons::paint_icon(painter, icon_rect, icon, text);
    painter.galley(
        egui::pos2(
            icon_rect.right() + icon_gap,
            button_rect.center().y - label_galley.rect.height() / 2.0,
        ),
        label_galley,
        text,
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    response
}

/// Material chevron for [`egui::ComboBox::icon`] — a down chevron that flips up
/// when the dropdown is open, replacing egui's tiny default triangle.
fn combo_chevron(
    ui: &egui::Ui,
    rect: egui::Rect,
    visuals: &egui::style::WidgetVisuals,
    is_open: bool,
) {
    let icon = if is_open {
        crate::gui::icons::Icon::ArrowUp
    } else {
        crate::gui::icons::Icon::ArrowDown
    };
    crate::gui::icons::paint_icon(ui.painter(), rect, icon, visuals.fg_stroke.color);
}

/// A themed [`egui::ComboBox`] that paints a Material chevron instead of egui's
/// default triangle. Drop-in replacement for `egui::ComboBox::from_id_salt(..)`.
pub fn combo(id_salt: impl std::hash::Hash) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id_salt).icon(combo_chevron)
}

/// Material chevron for collapsing headers — pass to `CollapsingHeader::icon` or
/// `CollapsingState::show_toggle_button`: right when closed, down when open.
pub fn collapsing_chevron(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let icon = if openness < 0.5 {
        crate::gui::icons::Icon::ArrowRight
    } else {
        crate::gui::icons::Icon::ArrowDown
    };
    let color = ui.style().interact(response).fg_stroke.color;
    crate::gui::icons::paint_icon(ui.painter(), response.rect, icon, color);
}
