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

use crate::gui::theme::{AppTheme, blend};
use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Shared Material surface for every modal in the egui application.
///
/// Content and sizing remain caller-owned, while elevation, scrim, border,
/// padding, and theme behavior stay impossible to accidentally fork.
pub fn material_modal<T>(
    ctx: &egui::Context,
    theme: &AppTheme,
    id: egui::Id,
    content: impl FnOnce(&mut egui::Ui) -> T,
) -> egui::ModalResponse<T> {
    egui::Modal::new(id)
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ctx, content)
}

/// Standard standalone dialog title for modals that do not expose a close
/// action, such as blocking progress and completion states.
pub fn dialog_title(ui: &mut egui::Ui, theme: &AppTheme, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(16.5)
            .strong()
            .color(theme.on_surface()),
    );
}

/// Standard wrapped supporting copy for a Material dialog.
pub fn dialog_body(ui: &mut egui::Ui, theme: &AppTheme, body: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(body)
                .size(12.5)
                .color(theme.on_surface_variant()),
        )
        .wrap(),
    );
}

/// Default hold before an item hands the slot over.
pub const CROSSFADE_HOLD_SECONDS: f64 = 1.5;
/// Crossfade length at each handover, inside the slot's own time.
const CROSSFADE_FADE_SECONDS: f64 = 0.25;

/// Which of `count` items a shared slot shows right now, and how opaque it is.
///
/// Use where several pieces of text compete for one line: each holds for
/// `CROSSFADE_HOLD_SECONDS` then fades through transparent into the next.
/// Every caller reads the same clock, so a table of rotating cells turns over
/// on one beat instead of shimmering cell by cell. Repaints are scheduled —
/// every frame while fading, then asleep until the next handover — so a static
/// moment costs nothing.
///
/// Apply the opacity with [`egui::Color32::gamma_multiply`].
pub fn crossfade_phase(ui: &egui::Ui, count: usize) -> (usize, f32) {
    crossfade_phase_held(ui, count, CROSSFADE_HOLD_SECONDS)
}

/// [`crossfade_phase`] with an explicit hold, for slots that carry longer text
/// and need more reading time than a table cell does.
pub fn crossfade_phase_held(ui: &egui::Ui, count: usize, hold: f64) -> (usize, f32) {
    if count < 2 {
        return (0, 1.0);
    }

    let slot = ui.input(|input| input.time) / hold;
    let index = (slot as usize) % count;
    let elapsed = slot.fract() * hold;
    let remaining = hold - elapsed;

    let fade_in = (elapsed / CROSSFADE_FADE_SECONDS).min(1.0);
    let fade_out = (remaining / CROSSFADE_FADE_SECONDS).min(1.0);

    let next = if elapsed < CROSSFADE_FADE_SECONDS || remaining < CROSSFADE_FADE_SECONDS {
        0.0
    } else {
        remaining - CROSSFADE_FADE_SECONDS
    };
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64(next));

    (index, fade_in.min(fade_out) as f32)
}

/// Baseline fraction of Google Sans Flex: how far below a centred galley's
/// centre its baseline sits, per em (ascent 0.9660 - height 1.2520 / 2).
const PROPORTIONAL_BASELINE_FRACTION: f32 = 0.3403;

/// Downward nudge that puts `size`-pt text on the same baseline as
/// `reference`-pt text when both are centre-aligned in one row.
fn baseline_nudge(reference: f32, size: f32) -> f32 {
    ((reference - size) * PROPORTIONAL_BASELINE_FRACTION).max(0.0)
}

/// Runs `contents` shifted down onto `reference`-pt text's baseline.
///
/// egui centres widgets by their box, so small text beside large text rides
/// high by the difference between their baseline-to-box-centre offsets — 2px
/// for 11.5pt copy next to an 18pt dialog title. Wrap the smaller *text* in
/// this; leave filled boxes (pills, chips, icon buttons) box-centred, which is
/// how they are meant to read next to a heading.
///
/// The margin is doubled on purpose: padding the top also makes the box
/// taller, and the row re-centres that taller box, handing half the padding
/// straight back. That holds while the wrapped content is the shorter side of
/// the row, which is the only situation this helper is for.
pub fn baseline_aligned<R>(
    ui: &mut egui::Ui,
    reference: f32,
    size: f32,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            top: (2.0 * baseline_nudge(reference, size)).round() as i8,
            ..Default::default()
        })
        .show(ui, contents)
        .inner
}

/// Title size for every settings modal header.
pub const DIALOG_TITLE_SIZE: f32 = 18.0;
/// Supporting-copy size that sits inline beside a dialog title.
pub const DIALOG_DESCRIPTION_SIZE: f32 = 11.5;

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
                .size(DIALOG_TITLE_SIZE)
                .strong()
                .color(theme.on_surface()),
        );
        // Description sits inline on the same row, just after the title, and
        // is nudged onto the title's baseline rather than its box centre.
        if let Some(desc) = description {
            ui.add_space(8.0);
            baseline_aligned(ui, DIALOG_TITLE_SIZE, DIALOG_DESCRIPTION_SIZE, |ui| {
                ui.label(
                    egui::RichText::new(desc)
                        .size(DIALOG_DESCRIPTION_SIZE)
                        .color(theme.on_surface_variant()),
                );
            });
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
        let btn = egui::Button::new(egui::RichText::new(label).color(text))
            .corner_radius(CornerRadius::same(corner_radius))
            .min_size(min_size);
        // egui positions a button's label via the parent layout's alignment, so a
        // button made wider than its text (min_size.x > 0, e.g. a full-width CTA
        // like "TẢI VỀ NGAY") would left-align the label. Center it both axes for
        // explicitly-sized buttons; tight buttons (zero min) are unaffected.
        if min_size.x > 0.0 {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add(btn)
            })
            .inner
        } else {
            ui.add(btn)
        }
    })
    .inner
}

/// A compact filled chip with a trailing close icon. Clicking anywhere on the
/// chip requests removal; callers own the collection mutation.
pub fn removable_chip(
    ui: &mut egui::Ui,
    label: &str,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
) -> egui::Response {
    let icon_px = crate::gui::icons::ICON_SM;
    let response = filled_button(ui, &format!("{label}      "), fill, text, corner_radius)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            response.rect.right() - icon_px * 0.5 - 8.0,
            response.rect.center().y,
        ),
        egui::vec2(icon_px, icon_px),
    );
    crate::gui::icons::paint_icon(
        ui.painter(),
        icon_rect,
        crate::gui::icons::Icon::Close,
        text,
    );
    response
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
    filled_icon_button_with_spacing(
        ui,
        icon,
        label,
        fill,
        text,
        corner_radius,
        FilledIconButtonLayout {
            minimum_horizontal_padding: 10.0,
            icon_gap: 6.0,
            text_style: egui::TextStyle::Button,
        },
    )
}

/// A horizontally compact [`filled_icon_button`] with the same control height.
///
/// Intended for dense launcher rows where reducing the gap and side padding is
/// preferable to shortening localized labels or shrinking the click target.
pub fn compact_filled_icon_button(
    ui: &mut egui::Ui,
    icon: crate::gui::icons::Icon,
    label: &str,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
) -> egui::Response {
    filled_icon_button_with_spacing(
        ui,
        icon,
        label,
        fill,
        text,
        corner_radius,
        FilledIconButtonLayout {
            minimum_horizontal_padding: 3.0,
            icon_gap: 3.0,
            text_style: egui::TextStyle::Button,
        },
    )
}

struct FilledIconButtonLayout {
    minimum_horizontal_padding: f32,
    icon_gap: f32,
    text_style: egui::TextStyle,
}

fn filled_icon_button_with_spacing(
    ui: &mut egui::Ui,
    icon: crate::gui::icons::Icon,
    label: &str,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
    layout: FilledIconButtonLayout,
) -> egui::Response {
    let label_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        layout.text_style.resolve(ui.style()),
        text,
    );
    let icon_size = crate::gui::icons::ICON_MD;
    let h_pad = ui
        .spacing()
        .button_padding
        .x
        .max(layout.minimum_horizontal_padding);
    let button_size = egui::vec2(
        h_pad + icon_size + layout.icon_gap + label_galley.rect.width() + h_pad,
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
            icon_rect.right() + layout.icon_gap,
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

/// A Material-style filled button with a label followed by a trailing icon.
///
/// Use this for actions that open a deeper surface, where a right chevron
/// communicates navigation more accurately than an immediate one-click action.
pub fn filled_trailing_icon_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: crate::gui::icons::Icon,
    fill: Color32,
    text: Color32,
    corner_radius: u8,
) -> egui::Response {
    let label_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::TextStyle::Button.resolve(ui.style()),
        text,
    );
    let icon_size = crate::gui::icons::ICON_SM;
    let icon_gap = 6.0;
    let h_pad = ui.spacing().button_padding.x.max(10.0);
    let button_size = egui::vec2(
        h_pad + label_galley.rect.width() + icon_gap + icon_size + h_pad,
        ui.spacing()
            .interact_size
            .y
            .max(label_galley.rect.height() + ui.spacing().button_padding.y * 2.0),
    );

    let (button_rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());
    let surface = if response.is_pointer_button_down_on() {
        blend(fill, text, 0.14)
    } else if response.hovered() {
        blend(fill, text, 0.08)
    } else {
        fill
    };

    let painter = ui.painter();
    painter.rect_filled(button_rect, CornerRadius::same(corner_radius), surface);
    painter.galley(
        egui::pos2(
            button_rect.left() + h_pad,
            button_rect.center().y - label_galley.rect.height() / 2.0,
        ),
        label_galley,
        text,
    );
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            button_rect.right() - h_pad - icon_size,
            button_rect.center().y - icon_size / 2.0,
        ),
        egui::vec2(icon_size, icon_size),
    );
    crate::gui::icons::paint_icon(painter, icon_rect, icon, text);

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
pub fn combo(id_salt: impl std::hash::Hash + std::fmt::Debug) -> egui::ComboBox {
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
    // The collapsing header's own icon rect is short, so `paint_icon` (which sizes
    // the glyph off the rect's MIN side) rendered a too-small chevron. Paint a
    // square sized off `icon_width` instead — the same metric egui uses for every
    // ComboBox arrow — so collapsing chevrons match the dropdown ones.
    let size = ui.spacing().icon_width;
    let rect = egui::Rect::from_center_size(response.rect.center(), egui::vec2(size, size));
    crate::gui::icons::paint_icon(ui.painter(), rect, icon, color);
}
