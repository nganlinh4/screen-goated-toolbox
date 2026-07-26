use crate::gui::icons::{self, Icon};
use crate::gui::locale::{UsageTipCategory, UsageTipSection, WorkspaceLocaleText};
use crate::gui::theme::{AppTheme, blend};
use eframe::egui;
use egui::text::{LayoutJob, TextFormat};

const DIALOG_MAX_WIDTH: f32 = 920.0;
const RAIL_BREAKPOINT: f32 = 760.0;
const RAIL_WIDTH: f32 = 216.0;

pub fn render_tips_modal(
    ctx: &egui::Context,
    text: &WorkspaceLocaleText,
    show_modal: &mut bool,
    selected_category: &mut UsageTipCategory,
) {
    if !*show_modal {
        return;
    }

    let Some(section) = resolve_section(text.tips_sections, *selected_category) else {
        *show_modal = false;
        return;
    };
    *selected_category = section.id;

    let theme = AppTheme::from_dark(ctx.global_style().visuals.dark_mode);
    let layout = dialog_layout(ctx.content_rect().size());
    let mut close_requested = false;

    let modal = egui::Modal::new(egui::Id::new("tips_modal"))
        .backdrop_color(theme.scrim_color())
        .frame(theme.dialog_frame())
        .show(ctx, |ui| {
            ui.set_width(layout.width);

            if render_header(ui, &theme, text) {
                close_requested = true;
            }
            ui.add_space(16.0);

            if layout.use_rail {
                render_rail_layout(
                    ui,
                    &theme,
                    text.tips_sections,
                    selected_category,
                    layout.body_height,
                );
            } else {
                render_compact_layout(
                    ui,
                    &theme,
                    text.tips_sections,
                    selected_category,
                    layout.body_height,
                );
            }

            ui.min_rect()
        });

    #[cfg(test)]
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("tips_modal_test_rect"), modal.inner);
    });

    if close_requested || modal.should_close() {
        *show_modal = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TipsDialogLayout {
    width: f32,
    body_height: f32,
    use_rail: bool,
}

fn dialog_layout(viewport: egui::Vec2) -> TipsDialogLayout {
    let width = (viewport.x - 80.0).clamp(360.0, DIALOG_MAX_WIDTH);
    let body_height = (viewport.y - 180.0).clamp(280.0, 450.0);
    TipsDialogLayout {
        width,
        body_height,
        use_rail: width >= RAIL_BREAKPOINT,
    }
}

fn resolve_section(
    sections: &[UsageTipSection],
    selected: UsageTipCategory,
) -> Option<&UsageTipSection> {
    sections
        .iter()
        .filter(|section| !section.tips.is_empty())
        .find(|section| section.id == selected)
        .or_else(|| sections.iter().find(|section| !section.tips.is_empty()))
}

fn render_header(ui: &mut egui::Ui, theme: &AppTheme, text: &WorkspaceLocaleText) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        let badge_size = 38.0;
        let (badge_rect, _) =
            ui.allocate_exact_size(egui::vec2(badge_size, badge_size), egui::Sense::hover());
        ui.painter().rect_filled(
            badge_rect,
            egui::CornerRadius::same(11),
            blend(theme.dialog_surface(), theme.warning(), 0.18),
        );
        icons::paint_icon(
            ui.painter(),
            egui::Rect::from_center_size(
                badge_rect.center(),
                egui::vec2(icons::ICON_XL, icons::ICON_XL),
            ),
            Icon::Lightbulb,
            theme.warning(),
        );

        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(text.tips_title)
                    .size(19.0)
                    .strong()
                    .color(theme.on_surface()),
            );
            ui.label(
                egui::RichText::new(text.tips_intro)
                    .size(11.5)
                    .color(theme.on_surface_variant()),
            );
        });

        let close_width = ui.spacing().interact_size.x;
        ui.add_space((ui.available_width() - close_width).max(0.0));
        if icons::icon_button(ui, Icon::Close).clicked() {
            close = true;
        }
    });
    close
}

fn render_rail_layout(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    sections: &[UsageTipSection],
    selected_category: &mut UsageTipCategory,
    body_height: f32,
) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(blend(theme.dialog_surface(), theme.neutral_fill(), 0.34))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(RAIL_WIDTH);
                ui.set_height(body_height);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    render_category_buttons(ui, theme, sections, selected_category, false);
                });
            });

        ui.add_space(10.0);
        let content_width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(content_width, body_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| render_selected_section(ui, theme, sections, *selected_category, body_height),
        );
    });
}

fn render_compact_layout(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    sections: &[UsageTipSection],
    selected_category: &mut UsageTipCategory,
    body_height: f32,
) {
    ui.horizontal_wrapped(|ui| {
        render_category_buttons(ui, theme, sections, selected_category, true);
    });
    ui.add_space(10.0);
    render_selected_section(
        ui,
        theme,
        sections,
        *selected_category,
        (body_height - 48.0).max(220.0),
    );
}

fn render_category_buttons(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    sections: &[UsageTipSection],
    selected_category: &mut UsageTipCategory,
    compact: bool,
) {
    ui.scope(|ui| {
        ui.visuals_mut().selection.bg_fill = blend(theme.dialog_surface(), theme.warning(), 0.20);
        ui.visuals_mut().selection.stroke.color = theme.warning();
        ui.spacing_mut().item_spacing.y = if compact { 6.0 } else { 8.0 };

        for section in sections.iter().filter(|section| !section.tips.is_empty()) {
            let selected = section.id == *selected_category;
            let button = egui::Button::selectable(
                selected,
                egui::RichText::new(section.title).size(if compact { 11.5 } else { 12.5 }),
            )
            .right_text(
                egui::RichText::new(section.tips.len().to_string())
                    .size(10.5)
                    .color(theme.on_surface_variant()),
            )
            .corner_radius(egui::CornerRadius::same(9))
            .min_size(if compact {
                egui::vec2(0.0, 32.0)
            } else {
                egui::vec2(RAIL_WIDTH, 42.0)
            });

            let response = ui
                .add(button)
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                *selected_category = section.id;
            }
        }
    });
}

fn render_selected_section(
    ui: &mut egui::Ui,
    theme: &AppTheme,
    sections: &[UsageTipSection],
    selected_category: UsageTipCategory,
    body_height: f32,
) {
    let Some(section) = resolve_section(sections, selected_category) else {
        return;
    };

    ui.horizontal(|ui| {
        let count_width = 24.0;
        let heading_width = (ui.available_width() - count_width - 8.0).max(120.0);
        ui.allocate_ui_with_layout(
            egui::vec2(heading_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(
                    egui::RichText::new(section.title)
                        .size(16.0)
                        .strong()
                        .color(theme.on_surface()),
                );
                ui.label(
                    egui::RichText::new(section.description)
                        .size(11.5)
                        .color(theme.on_surface_variant()),
                );
            },
        );
        ui.add_space(8.0);
        ui.allocate_ui_with_layout(
            egui::vec2(count_width, 0.0),
            egui::Layout::right_to_left(egui::Align::Min),
            |ui| {
                ui.label(
                    egui::RichText::new(section.tips.len().to_string())
                        .size(11.0)
                        .strong()
                        .color(theme.warning()),
                );
            },
        );
    });
    ui.add_space(10.0);

    egui::ScrollArea::vertical()
        .id_salt(("tips_section_scroll", section.id.stable_id()))
        .max_height((body_height - 48.0).max(180.0))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            for tip in section.tips {
                render_tip_note(ui, theme, tip.text);
            }
        });
}

fn render_tip_note(ui: &mut egui::Ui, theme: &AppTheme, text: &str) {
    let frame = egui::Frame::new()
        .fill(blend(theme.dialog_surface(), theme.warning(), 0.035))
        .stroke(egui::Stroke::new(
            1.0,
            blend(theme.dialog_surface(), theme.on_surface_variant(), 0.24),
        ))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 10,
            bottom: 10,
        });

    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(format_tip_with_emphasis(
            text,
            theme.on_surface_variant(),
            theme.controller_mode_accent(),
        )));
    });
}

fn format_tip_with_emphasis(
    text: &str,
    regular: egui::Color32,
    emphasis: egui::Color32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (index, segment) in text.split("**").enumerate() {
        job.append(
            segment,
            0.0,
            TextFormat {
                font_id: egui::FontId::proportional(13.0),
                color: if index % 2 == 0 { regular } else { emphasis },
                ..Default::default()
            },
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};

    #[test]
    fn dialog_fits_the_minimum_window_and_has_a_compact_fallback() {
        let minimum = dialog_layout(egui::vec2(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT));
        assert!(minimum.use_rail);
        assert!(minimum.width + 80.0 <= MIN_WINDOW_WIDTH);
        assert!(minimum.body_height + 180.0 <= MIN_WINDOW_HEIGHT);

        let narrow = dialog_layout(egui::vec2(700.0, 560.0));
        assert!(!narrow.use_rail);
        assert!(narrow.width + 80.0 <= 700.0);
    }

    #[test]
    fn unavailable_selection_falls_back_to_first_nonempty_category() {
        let sections = [
            UsageTipSection {
                id: UsageTipCategory::CaptureShortcuts,
                title: "Empty",
                description: "Empty",
                tips: &[],
            },
            UsageTipSection {
                id: UsageTipCategory::ResultsRecovery,
                title: "Results",
                description: "Results",
                tips: &[crate::gui::locale::UsageTip {
                    id: "one",
                    text: "One",
                }],
            },
        ];

        assert_eq!(
            resolve_section(&sections, UsageTipCategory::CreativeTools).map(|section| section.id),
            Some(UsageTipCategory::ResultsRecovery)
        );
    }

    #[test]
    fn modal_rect_stays_inside_the_minimum_viewport_for_every_locale() {
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT),
        );

        for language in ["en", "vi", "ko"] {
            let context = egui::Context::default();
            crate::gui::configure_fonts(&context);
            AppTheme::apply_global_style(&context, false);
            let locale = crate::gui::locale::LocaleText::get(language);
            let mut show = true;
            let mut selected = UsageTipCategory::default();

            for _ in 0..2 {
                let _ = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ui| {
                        render_tips_modal(ui.ctx(), &locale.workspace, &mut show, &mut selected);
                    },
                );
            }

            let rect = context.data(|data| {
                data.get_temp::<egui::Rect>(egui::Id::new("tips_modal_test_rect"))
                    .expect("tips modal rect")
            });
            assert!(
                screen.contains_rect(rect),
                "{language} modal escaped viewport: {rect:?}"
            );
        }
    }

    #[test]
    fn emphasis_markers_do_not_leak_into_rendered_text() {
        let job = format_tip_with_emphasis(
            "Use **hidden gesture** here.",
            egui::Color32::GRAY,
            egui::Color32::YELLOW,
        );
        assert_eq!(job.text, "Use hidden gesture here.");
        assert!(!job.text.contains("**"));
        assert_eq!(job.sections.len(), 3);
    }
}
