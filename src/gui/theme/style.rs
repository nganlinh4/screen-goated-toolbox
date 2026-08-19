//! The one-time global egui style pass.
//!
//! Split out of the theme module because it is a long, flat sequence of
//! assignments with a different rhythm from the colour accessors beside it.

use super::metrics::CONTROL_HEIGHT;
use super::{AppTheme, blend};
use eframe::egui::{self, Color32, Context, CornerRadius, CursorIcon, Shadow, Stroke, Visuals};

impl AppTheme {
    /// Build and install a Material-style global egui style for the whole app.
    ///
    /// This is the single highest-leverage styling hook: it replaces egui's flat
    /// `Visuals::dark()/light()` defaults with rounded widgets, Material state
    /// layers (hover/press), an accent selection color, rounded + shadowed
    /// windows / popups / menus, and semantic colors — so every standard widget
    /// (buttons, combo boxes, checkboxes, text fields, sliders, scrollbars,
    /// popups) matches the Material confirm-dialog look without touching each
    /// call site. Fonts are configured separately, so they are preserved.
    ///
    /// Call once at startup and again on every effective theme change.
    pub fn apply_global_style(ctx: &Context, dark: bool) {
        let theme = AppTheme::from_dark(dark);
        let mut style = (*ctx.global_style()).clone();
        let mut v = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        let text = theme.on_surface();
        let hairline = theme.pick(
            Color32::from_rgb(44, 48, 59),
            Color32::from_rgb(224, 227, 233),
        );
        // Resting surface for filled controls (buttons, combos, sliders).
        let control = theme.pick(
            Color32::from_rgb(46, 50, 62),
            Color32::from_rgb(228, 230, 238),
        );

        // Base surfaces & accents.
        v.panel_fill = theme.pick(
            Color32::from_rgb(22, 24, 30),
            Color32::from_rgb(244, 245, 248),
        );
        v.faint_bg_color = theme.pick(
            Color32::from_rgb(32, 36, 45),
            Color32::from_rgb(236, 238, 243),
        );
        v.extreme_bg_color = theme.pick(
            Color32::from_rgb(30, 33, 42),
            Color32::from_rgb(252, 252, 254),
        );
        v.code_bg_color = theme.pick(
            Color32::from_rgb(30, 33, 42),
            Color32::from_rgb(236, 238, 243),
        );
        v.hyperlink_color = theme.pick(
            Color32::from_rgb(132, 176, 255),
            Color32::from_rgb(40, 95, 200),
        );
        v.warn_fg_color = theme.warning();
        v.error_fg_color = theme.danger_text();

        // Selection / focus accent (text selection + selectable_label highlight).
        v.selection.bg_fill = theme.pick(
            Color32::from_rgba_unmultiplied(74, 118, 208, 115),
            Color32::from_rgba_unmultiplied(48, 100, 190, 70),
        );
        v.selection.stroke = Stroke::new(
            1.0,
            theme.pick(
                Color32::from_rgb(150, 185, 255),
                Color32::from_rgb(40, 90, 180),
            ),
        );

        // Windows / popups / menus: rounded with soft elevation.
        v.window_fill = theme.dialog_surface();
        v.window_stroke = Stroke::new(
            1.0,
            theme.pick(
                Color32::from_rgb(58, 64, 78),
                Color32::from_rgb(228, 230, 236),
            ),
        );
        v.window_corner_radius = CornerRadius::same(16);
        v.window_shadow = Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 0,
            color: theme.pick(
                Color32::from_black_alpha(140),
                Color32::from_black_alpha(50),
            ),
        };
        v.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: theme.pick(
                Color32::from_black_alpha(120),
                Color32::from_black_alpha(40),
            ),
        };
        v.menu_corner_radius = CornerRadius::same(12);

        // Interactive widget states with Material state layers.
        let radius = CornerRadius::same(10);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, hairline); // separators / groups
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.noninteractive.corner_radius = radius;
        for (state, t) in [
            (&mut v.widgets.inactive, 0.0_f32),
            (&mut v.widgets.hovered, 0.08),
            (&mut v.widgets.active, 0.14),
            (&mut v.widgets.open, 0.10),
        ] {
            let fill = blend(control, text, t);
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.fg_stroke = Stroke::new(1.0, text);
            state.corner_radius = radius;
        }
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, blend(control, text, 0.20));
        v.widgets.hovered.expansion = 1.0;
        v.widgets.active.bg_stroke = Stroke::new(1.0, theme.accent_fill());
        v.widgets.active.expansion = 1.0;
        v.widgets.open.bg_stroke = Stroke::new(
            1.0,
            theme.pick(
                Color32::from_rgb(70, 76, 90),
                Color32::from_rgb(205, 208, 216),
            ),
        );

        // Controls feel.
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.interact_cursor = Some(CursorIcon::PointingHand);

        style.visuals = v;
        // Slightly roomier buttons without reflowing the dense layouts much.
        style.spacing.button_padding = egui::vec2(7.0, 3.0);

        // Every row starts at the design system's control height.
        //
        // `ui.horizontal` centres each widget against the row height known when
        // that widget is added, and a row starts out `interact_size.y` tall. A
        // label written before a taller control is therefore centred in the
        // *starting* height and never re-centred once the control grows the
        // row, so it renders `(control_height - interact_size.y) / 2` too high.
        // With egui's default 18 against this app's 22px pills that was a
        // uniform 2px lift on every label-then-control row.
        //
        // Starting the row at the control height removes the gap at the source:
        // both sides now centre against the same number, whatever their order.
        style.spacing.interact_size.y = CONTROL_HEIGHT;

        // egui turns on these red debug overlays by default in debug builds
        // (`cargo run`): `warn_if_rect_changes_id` paints a 2px red outline on
        // every widget whose rect changed id, and `show_unaligned` highlights
        // sub-pixel-misaligned widgets. They flicker over the UI on interaction
        // and are dev-only diagnostics we don't use.
        #[cfg(debug_assertions)]
        {
            style.debug.warn_if_rect_changes_id = false;
            style.debug.show_unaligned = false;
            style.debug.show_interactive_widgets = false;
            style.debug.show_widget_hits = false;
        }

        ctx.set_global_style(style);
    }
}
