//! Centralized design tokens for the egui settings UI.
//!
//! Build an `AppTheme` once per render (from the active visuals) and read
//! semantic colors / frames from it, instead of hand-branching
//! `if ui.visuals().dark_mode { rgb_a } else { rgb_b }` at every call site.
//! Each token resolves dark vs light internally, so the values live in exactly
//! one place. Only patterns that actually repeat across the UI are tokenized.

use eframe::egui::{
    self, Color32, Context, CornerRadius, CursorIcon, Frame, Margin, Shadow, Stroke, Ui, Visuals,
};

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

    // --- Brand / launcher accents (title bar + footer) ----------------------

    /// PromptDJ launcher accent — violet (its on-brand #9900ff family).
    pub fn accent_prompt_dj(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(150, 118, 245),
            Color32::from_rgb(124, 58, 237),
        )
    }

    /// Download Manager launcher accent — red.
    pub fn accent_download(&self) -> Color32 {
        self.pick(Color32::from_rgb(224, 96, 96), Color32::from_rgb(216, 62, 62))
    }

    /// Screen Record launcher accent — blue (its design-system primary).
    pub fn accent_screen_record(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(74, 130, 200),
            Color32::from_rgb(37, 99, 235),
        )
    }

    /// Help Assistant launcher accent — teal (distinct from PromptDJ's violet).
    pub fn accent_help(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(30, 160, 148),
            Color32::from_rgb(15, 140, 130),
        )
    }

    /// Pointer Gallery footer launcher — green (distinct from Screen Record's blue).
    pub fn launch_pointer(&self) -> Color32 {
        self.pick(Color32::from_rgb(56, 168, 112), Color32::from_rgb(34, 150, 94))
    }

    /// Translation Gummy footer launcher — rose.
    pub fn launch_translation(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(200, 85, 110),
            Color32::from_rgb(235, 120, 145),
        )
    }

    /// TTS Playground footer launcher — warm amber (dark) / terracotta (light).
    pub fn launch_tts(&self) -> Color32 {
        self.pick(Color32::from_rgb(237, 137, 54), Color32::from_rgb(194, 65, 12))
    }

    // --- Global settings modal-open buttons ---------------------------------

    /// "Usage statistics" modal-open button fill — teal.
    pub fn btn_stats(&self) -> Color32 {
        self.pick(Color32::from_rgb(50, 100, 110), Color32::from_rgb(90, 160, 170))
    }

    /// "TTS settings" modal-open button fill — purple.
    pub fn btn_tts_settings(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(100, 80, 120),
            Color32::from_rgb(180, 140, 200),
        )
    }

    /// "Downloaded tools" modal-open button fill — blue.
    pub fn btn_tools(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(60, 90, 140),
            Color32::from_rgb(100, 140, 200),
        )
    }

    /// "Model priority" modal-open button fill — amber/brown.
    pub fn btn_priority(&self) -> Color32 {
        self.pick(Color32::from_rgb(120, 88, 50), Color32::from_rgb(196, 142, 73))
    }

    // --- Preset hotkey controls ---------------------------------------------

    /// "Add hotkey" button fill — teal.
    pub fn hotkey_add_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(50, 110, 120),
            Color32::from_rgb(100, 170, 180),
        )
    }

    /// "Cancel hotkey recording" button fill — muted red.
    pub fn hotkey_cancel_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(120, 60, 60),
            Color32::from_rgb(220, 150, 150),
        )
    }

    /// Existing-hotkey removable chip fill — violet.
    pub fn hotkey_item_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(90, 70, 130),
            Color32::from_rgb(170, 150, 200),
        )
    }

    /// "Restore default preset" button fill — muted violet (secondary action).
    pub fn restore_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(80, 70, 100),
            Color32::from_rgb(180, 170, 200),
        )
    }

    // --- Controller-mode description panel ----------------------------------

    /// Background of the controller / realtime mode description panel.
    pub fn controller_mode_bg(&self) -> Color32 {
        self.pick(
            Color32::from_rgba_unmultiplied(60, 70, 85, 180),
            Color32::from_rgba_unmultiplied(230, 235, 245, 255),
        )
    }

    /// Heading accent of the controller / realtime mode description panel.
    pub fn controller_mode_accent(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(130, 180, 230),
            Color32::from_rgb(70, 120, 180),
        )
    }

    // --- Sidebar -------------------------------------------------------------

    /// Faint background chip painted behind a preset row that has a hotkey bound.
    pub fn hotkey_chip_bg(&self) -> Color32 {
        self.pick(
            Color32::from_rgba_unmultiplied(40, 150, 130, 70),
            Color32::from_rgb(200, 235, 220),
        )
    }

    // --- Node graph (egui-snarl chain editor) -------------------------------

    /// Input pin dot — green (text connections). Theme-independent.
    pub fn pin_input(&self) -> Color32 {
        Color32::from_rgb(100, 200, 100)
    }

    /// Output pin dot — blue. Theme-independent.
    pub fn pin_output(&self) -> Color32 {
        Color32::from_rgb(100, 150, 255)
    }

    /// "Special" node header title text — orange.
    pub fn node_special_title(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(255, 200, 100),
            Color32::from_rgb(200, 100, 0),
        )
    }

    /// Small in-node button fill (e.g. the "+ Language" tag button) — teal.
    pub fn node_button_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(50, 100, 110),
            Color32::from_rgb(100, 160, 170),
        )
    }

    // --- Window controls (title bar min / max / close) ----------------------

    /// Hover wash behind the minimize / maximize caption buttons.
    pub fn window_control_hover(&self) -> Color32 {
        self.pick(Color32::from_gray(60), Color32::from_gray(220))
    }

    /// Hover wash behind the close caption button — Windows-red. Theme-independent.
    pub fn window_control_close_hover(&self) -> Color32 {
        Color32::from_rgb(232, 17, 35)
    }

    // --- Dialog (Material 3) ------------------------------------------------

    /// Scrim painted behind a modal dialog. A touch stronger than egui's
    /// default backdrop so the dialog reads as the clear focus.
    pub fn scrim_color(&self) -> Color32 {
        self.pick(Color32::from_black_alpha(150), Color32::from_black_alpha(96))
    }

    /// Raised container surface for dialogs (one elevation step above `card_bg`).
    pub fn dialog_surface(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(37, 41, 52),
            Color32::from_rgb(252, 252, 255),
        )
    }

    /// Primary on-surface text — dialog headline and emphasized lines.
    pub fn on_surface(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(228, 231, 238),
            Color32::from_rgb(26, 28, 33),
        )
    }

    /// Muted on-surface text — supporting body copy.
    pub fn on_surface_variant(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(176, 182, 196),
            Color32::from_rgb(92, 97, 110),
        )
    }

    /// Low-emphasis tonal button fill (e.g. a dialog's Cancel action).
    pub fn neutral_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(55, 60, 74),
            Color32::from_rgb(232, 234, 241),
        )
    }

    /// High-emphasis primary fill (e.g. a non-destructive confirm action).
    pub fn accent_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(66, 110, 200),
            Color32::from_rgb(48, 100, 190),
        )
    }

    /// High-emphasis destructive fill (e.g. a dialog's Delete action).
    pub fn danger_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(201, 74, 74),
            Color32::from_rgb(211, 47, 47),
        )
    }

    /// Text/icon color drawn on top of `danger_fill` / `accent_fill`.
    pub fn on_accent(&self) -> Color32 {
        Color32::WHITE
    }

    /// Material-style container for a modal dialog: raised surface, rounded
    /// corners, a soft elevation shadow and roomy padding.
    pub fn dialog_frame(&self) -> Frame {
        Frame::new()
            .fill(self.dialog_surface())
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(20))
            .stroke(Stroke::new(
                1.0,
                self.pick(
                    Color32::from_rgb(58, 64, 78),
                    Color32::from_rgb(228, 230, 236),
                ),
            ))
            .shadow(Shadow {
                offset: [0, 4],
                blur: 20,
                spread: 0,
                color: self.pick(
                    Color32::from_black_alpha(130),
                    Color32::from_black_alpha(55),
                ),
            })
    }

    // --- Semantic status colors ---------------------------------------------

    /// Warning / caution accent (e.g. "update available", recording prompt).
    pub fn warning(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(240, 180, 90),
            Color32::from_rgb(196, 120, 20),
        )
    }

    /// Success / healthy accent (e.g. "up to date", admin running).
    pub fn success(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(120, 200, 130),
            Color32::from_rgb(34, 139, 34),
        )
    }

    /// Destructive / error text accent (distinct from the filled `danger_fill`).
    pub fn danger_text(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(240, 122, 122),
            Color32::from_rgb(200, 50, 50),
        )
    }

    /// Saturated amber fill for a secondary/cautionary action — readable with
    /// white text in both themes, and clearly distinct from the red `danger_fill`.
    pub fn warning_fill(&self) -> Color32 {
        self.pick(
            Color32::from_rgb(198, 128, 44),
            Color32::from_rgb(194, 110, 20),
        )
    }

    // --- Global style -------------------------------------------------------

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
        let mut v = if dark { Visuals::dark() } else { Visuals::light() };

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
            color: theme.pick(Color32::from_black_alpha(140), Color32::from_black_alpha(50)),
        };
        v.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: theme.pick(Color32::from_black_alpha(120), Color32::from_black_alpha(40)),
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
        // egui enables these red debug overlays by default in debug builds
        // (`cargo run`); they flicker over widgets on interaction. Keep the UI clean.
        style.debug.show_unaligned = false;
        style.debug.show_interactive_widgets = false;
        style.debug.show_widget_hits = false;
        // Paints a 2px red outline on every widget whose rect moved since the
        // last frame — i.e. red boxes everywhere while scrolling. On by default
        // in debug builds; kill it so the UI stays clean.
        style.debug.warn_if_rect_changes_id = false;

        ctx.set_global_style(style);
    }
}

/// Linear blend from `a` toward `b` by `t` (0.0 = `a`, 1.0 = `b`).
///
/// Used to build Material state layers — overlay the on-color over a fill to get
/// hover (≈8%) and pressed (≈14%) variants that read correctly in both themes.
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}
