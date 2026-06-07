use crate::gui::icons::{Icon, paint_icon};
use crate::gui::locale::LocaleText;
use crate::gui::theme::AppTheme;
use crate::gui::widgets::filled_icon_button;
use eframe::egui;
use egui::text::{LayoutJob, TextFormat};

pub fn render_footer(
    ui: &mut egui::Ui,
    text: &LocaleText,
    current_tip: String,
    tip_alpha: f32,
    tip_scroll: f32,
    toggles: FooterToggles<'_>,
) {
    let FooterToggles {
        show_modal,
        show_pointer_gallery,
        show_translation_gummy,
        show_tts_playground,
        show_download,
    } = toggles;
    ui.horizontal(|ui| {
        // 1. Left Side: the mini-app launch buttons.
        let theme = AppTheme::from_ui(ui);
        let is_dark = ui.visuals().dark_mode;
        // Bright accent fills read better with near-black labels in dark mode.
        let btn_text = if is_dark {
            egui::Color32::from_rgb(22, 22, 26)
        } else {
            egui::Color32::WHITE
        };
        ui.spacing_mut().item_spacing.x = 6.0;

        // Pointer Gallery — green
        if filled_icon_button(
            ui,
            Icon::Pointer,
            text.pointer_gallery_btn,
            theme.launch_pointer(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_pointer_gallery = true;
        }
        // Translation Gummy — rose
        if filled_icon_button(
            ui,
            Icon::BreakfastDining,
            text.translation_gummy_btn,
            theme.launch_translation(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_translation_gummy = true;
        }
        // TTS Playground — amber / terracotta
        if filled_icon_button(
            ui,
            Icon::Speaker,
            text.tts_playground_btn,
            theme.launch_tts(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_tts_playground = true;
        }
        // PromptDJ — violet
        if filled_icon_button(
            ui,
            Icon::Album,
            text.prompt_dj_btn,
            theme.accent_prompt_dj(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::prompt_dj::show_prompt_dj();
        }
        // Download Manager — red
        if filled_icon_button(
            ui,
            Icon::Movie,
            text.download_feature_btn,
            theme.accent_download(),
            btn_text,
            6,
        )
        .clicked()
        {
            *show_download = true;
        }
        // Screen Record — blue
        if filled_icon_button(
            ui,
            Icon::Videocam,
            text.screen_record_btn,
            theme.accent_screen_record(),
            btn_text,
            6,
        )
        .clicked()
        {
            crate::overlay::screen_record::show_screen_record();
        }

        ui.add_space(10.0);

        // 2. Tips: an EXPANDING display window — a minimum width that grows to
        // fit the tip, up to the free space. The tip fades in, holds, and ONLY if
        // it's too long to fit does it slide left to reveal the overflow, then
        // fades out (driven by `tip_alpha` / `tip_scroll` from `update_tips_logic`).
        // Text is clipped to the window, so long tips never push the layout around.
        let tip_color = ui.visuals().text_color().linear_multiply(tip_alpha);
        let icon_color =
            egui::Color32::from_rgba_unmultiplied(255, 200, 50, (tip_alpha * 255.0) as u8);
        let icon_size = crate::gui::icons::ICON_SM;
        let icon_spacing = 4.0;
        let is_dark_mode = ui.visuals().dark_mode;
        let layout_job = format_footer_tip(&current_tip, tip_color, is_dark_mode, tip_alpha);
        let text_galley = ui.painter().layout_job(layout_job);

        // Window = the tip's width, but AT LEAST `TIP_WINDOW_MIN` (so the region
        // doesn't jump as tips cycle) and never beyond the free space. So a tip
        // that fits in the available room is shown in full (overflow = 0 → it
        // never slides); only genuinely-too-long tips slide.
        const TIP_WINDOW_MIN: f32 = 480.0;
        let avail_for_text = (ui.available_width() - 10.0 - icon_size - icon_spacing).max(40.0);
        let min_window = TIP_WINDOW_MIN.min(avail_for_text);
        let window_w = text_galley.rect.width().clamp(min_window, avail_for_text);
        let region_w = icon_size + icon_spacing + window_w;

        let (response, painter) = ui.allocate_painter(
            egui::vec2(region_w + 8.0, ui.available_height().max(18.0)),
            egui::Sense::click(),
        );
        let rect = response.rect;
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.center().y - icon_size / 2.0),
            egui::vec2(icon_size, icon_size),
        );
        paint_icon(&painter, icon_rect, Icon::Lightbulb, icon_color);

        // Clip the text to its window and slide it left by `tip_scroll * overflow`.
        let win_left = icon_rect.right() + icon_spacing;
        let win_rect = egui::Rect::from_min_size(
            egui::pos2(win_left, rect.top()),
            egui::vec2(window_w, rect.height()),
        );
        let overflow = (text_galley.rect.width() - window_w).max(0.0);
        let text_pos = egui::pos2(
            win_left - tip_scroll * overflow,
            rect.center().y - text_galley.rect.height() / 2.0,
        );
        painter
            .with_clip_rect(win_rect)
            .galley(text_pos, text_galley, egui::Color32::WHITE);

        if response.on_hover_text(text.tips_click_hint).clicked() {
            *show_modal = true;
        }
    });
}

pub struct FooterToggles<'a> {
    pub show_modal: &'a mut bool,
    pub show_pointer_gallery: &'a mut bool,
    pub show_translation_gummy: &'a mut bool,
    pub show_tts_playground: &'a mut bool,
    pub show_download: &'a mut bool,
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
