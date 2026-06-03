use crate::gui::icons::{Icon, paint_icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::filled_button_sized;
use eframe::egui;
use egui::text::{LayoutJob, TextFormat};

/// Footer quick-launch button: a Material `filled_button` (token fill + state
/// layers) with the original leading vector icon painted over a reserved label
/// column. Uses the global button padding so its height matches the title-bar
/// and settings buttons, plus the custom PointingHand hover cursor.
fn render_launch_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: Icon,
    btn_color: egui::Color32,
    btn_bg: egui::Color32,
) -> egui::Response {
    let icon_sz = 12.0;
    let icon_gap = 4.0;
    // Inherit the global button padding so footer buttons match the title-bar /
    // settings buttons (~21px tall) instead of the old cramped 1px height.
    let h_pad = ui.spacing().button_padding.x;

    // Reserve a leading column for the vector icon by prefixing the label with
    // spaces whose advance width matches `icon_sz + icon_gap`; the icon is then
    // painted over that gap. This keeps the original [h_pad | icon | gap |
    // label | h_pad] geometry while letting `filled_button` own the surface.
    // Measure with the button's own body font so the reserve tracks the label.
    let label_font = egui::TextStyle::Button.resolve(ui.style());
    let space_w = ui
        .painter()
        .layout_no_wrap(" ".to_string(), label_font, btn_color)
        .rect
        .width()
        .max(0.1);
    let lead_spaces = ((icon_sz + icon_gap) / space_w).ceil() as usize;
    let padded_label = format!("{}{}", " ".repeat(lead_spaces), label);

    // Render through the shared filled_button so the fill comes from the theme
    // token and hover/press gain Material state layers. Force the original
    // h_pad / v_pad button padding so width and height match the prior look.
    let btn_response =
        filled_button_sized(ui, &padded_label, btn_bg, btn_color, 6, egui::Vec2::ZERO);

    let btn_rect = btn_response.rect;
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(btn_rect.left() + h_pad, btn_rect.center().y - icon_sz / 2.0),
        egui::vec2(icon_sz, icon_sz),
    );
    paint_icon(ui.painter(), icon_rect, icon, btn_color);

    if btn_response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    btn_response
}

pub fn render_footer(
    ui: &mut egui::Ui,
    text: &LocaleText,
    current_tip: String,
    tip_alpha: f32,
    toggles: FooterToggles<'_>,
) {
    let FooterToggles {
        show_modal,
        show_translation_gummy,
        show_tts_playground,
        show_pointer_gallery,
    } = toggles;
    ui.horizontal(|ui| {
        // 1. Left Side: Quick launch buttons
        let theme = AppTheme::from_ui(ui);
        let is_dark = ui.visuals().dark_mode;
        // Launcher fills are light in dark mode, so near-black label text reads
        // better there; white in light mode where the fills are deeper. This is
        // also the on-color used by `filled_button` for its hover/press layers.
        let btn_text = if is_dark {
            egui::Color32::from_rgb(22, 22, 26)
        } else {
            egui::Color32::WHITE
        };

        // Pointer gallery — green (distinct from Screen Record's blue).
        if render_launch_button(
            ui,
            text.pointer_gallery_btn,
            Icon::Pointer,
            btn_text,
            theme.launch_pointer(),
        )
        .clicked()
        {
            *show_pointer_gallery = true;
        }

        // Translation Gummy — rose accent.
        if render_launch_button(
            ui,
            text.translation_gummy_btn,
            Icon::Speaker,
            btn_text,
            theme.launch_translation(),
        )
        .clicked()
        {
            *show_translation_gummy = true;
        }

        // Matches the TTS Playground mini-app accent: warm amber (dark) /
        // terracotta (light).
        if render_launch_button(
            ui,
            text.tts_playground_btn,
            Icon::Speaker,
            btn_text,
            theme.launch_tts(),
        )
        .clicked()
        {
            *show_tts_playground = true;
        }

        ui.add_space(8.0);

        // 2. Center: Tips (Takes remaining space)
        let tip_color = ui.visuals().text_color().linear_multiply(tip_alpha);
        let icon_color =
            egui::Color32::from_rgba_unmultiplied(255, 200, 50, (tip_alpha * 255.0) as u8);

        let icon_size = 14.0;
        let icon_spacing = 4.0;

        let is_dark_mode = ui.visuals().dark_mode;
        let layout_job = format_footer_tip(&current_tip, tip_color, is_dark_mode, tip_alpha);
        let text_galley = ui.painter().layout_job(layout_job);
        let total_width = icon_size + icon_spacing + text_galley.rect.width();

        // Reserve ~180px for the right-side admin text
        let available = ui.available_width() - 180.0;
        let tip_width = total_width.min(available.max(100.0));

        let (response, painter) = ui.allocate_painter(
            egui::vec2(tip_width + 8.0, ui.available_height().max(18.0)),
            egui::Sense::click(),
        );
        let rect = response.rect;

        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.center().y - icon_size / 2.0),
            egui::vec2(icon_size, icon_size),
        );
        paint_icon(&painter, icon_rect, Icon::Lightbulb, icon_color);

        let text_pos = egui::pos2(
            icon_rect.right() + icon_spacing,
            rect.center().y - text_galley.rect.height() / 2.0,
        );
        painter.galley(text_pos, text_galley, egui::Color32::WHITE);

        if response.on_hover_text(text.tips_click_hint).clicked() {
            *show_modal = true;
        }

        // 3. Right Side: Admin Text (moved from left)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_admin = cfg!(target_os = "windows") && crate::gui::utils::is_running_as_admin();
            let footer_text = if is_admin {
                // Running-as-admin is a healthy/success status.
                egui::RichText::new(text.footer_admin_running)
                    .size(11.0)
                    .color(theme.success())
            } else {
                // Idle / non-admin is low-emphasis supporting copy.
                egui::RichText::new(text.footer_admin_text)
                    .size(11.0)
                    .color(theme.on_surface_variant())
            };
            ui.label(footer_text);
        });
    });
}

pub struct FooterToggles<'a> {
    pub show_modal: &'a mut bool,
    pub show_translation_gummy: &'a mut bool,
    pub show_tts_playground: &'a mut bool,
    pub show_pointer_gallery: &'a mut bool,
}

// Helper function to format footer tip with bold text
fn format_footer_tip(
    text: &str,
    base_color: egui::Color32,
    is_dark_mode: bool,
    alpha_factor: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();

    // Color scheme for bold text
    let bold_color = if is_dark_mode {
        egui::Color32::from_rgb(150, 200, 255) // Soft cyan for dark mode
    } else {
        egui::Color32::from_rgb(40, 100, 180) // Dark blue for light mode
    };

    // Apply alpha to colors
    let regular_color = egui::Color32::from_rgba_unmultiplied(
        base_color.r(),
        base_color.g(),
        base_color.b(),
        (base_color.a() as f32 * alpha_factor) as u8,
    );

    let bold_color_with_alpha = egui::Color32::from_rgba_unmultiplied(
        bold_color.r(),
        bold_color.g(),
        bold_color.b(),
        (255.0 * alpha_factor) as u8,
    );

    // Create text format
    let text_format = TextFormat {
        font_id: egui::FontId::proportional(11.0),
        color: regular_color,
        ..Default::default()
    };

    // Parse text for **bold** markers
    let mut current_text = String::new();
    let mut chars = text.chars().peekable();
    let mut is_bold = false;

    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            // Found ** marker
            chars.next(); // consume second *

            if !current_text.is_empty() {
                // Append accumulated text
                let mut fmt = text_format.clone();
                if is_bold {
                    fmt.color = bold_color_with_alpha;
                }
                job.append(&current_text, 0.0, fmt);
                current_text.clear();
            }

            is_bold = !is_bold;
        } else {
            current_text.push(ch);
        }
    }

    // Append remaining text
    if !current_text.is_empty() {
        let mut fmt = text_format.clone();
        if is_bold {
            fmt.color = bold_color_with_alpha;
        }
        job.append(&current_text, 0.0, fmt);
    }

    job
}
