use eframe::egui;

#[derive(Clone, Copy)]
pub(super) struct RealtimeEguiTheme {
    pub surface: egui::Color32,
    pub header: egui::Color32,
    pub control: egui::Color32,
    pub control_hover: egui::Color32,
    pub text: egui::Color32,
    pub muted: egui::Color32,
    pub border: egui::Color32,
    pub primary: egui::Color32,
    pub secondary: egui::Color32,
    pub warning: egui::Color32,
}

impl RealtimeEguiTheme {
    pub fn new(dark: bool) -> Self {
        let (pr, pg, pb) = crate::overlay::utils::ACCENT_PRIMARY_RGB;
        if dark {
            Self {
                surface: egui::Color32::from_rgba_premultiplied(28, 27, 31, 242),
                header: egui::Color32::from_rgba_premultiplied(44, 44, 48, 210),
                control: egui::Color32::from_rgba_premultiplied(44, 44, 44, 220),
                control_hover: egui::Color32::from_rgba_premultiplied(pr, pg, pb, 55),
                text: egui::Color32::from_rgb(230, 225, 229),
                muted: egui::Color32::from_rgb(147, 143, 153),
                border: egui::Color32::from_rgba_premultiplied(0, 200, 255, 70),
                primary: egui::Color32::from_rgb(pr, pg, pb),
                secondary: egui::Color32::from_rgb(41, 121, 255),
                warning: egui::Color32::from_rgb(255, 180, 100),
            }
        } else {
            Self {
                surface: egui::Color32::from_rgba_premultiplied(254, 247, 255, 242),
                header: egui::Color32::from_rgba_premultiplied(255, 255, 255, 230),
                control: egui::Color32::from_rgba_premultiplied(234, 234, 234, 220),
                control_hover: egui::Color32::from_rgba_premultiplied(pr, pg, pb, 30),
                text: egui::Color32::from_rgb(28, 27, 31),
                muted: egui::Color32::from_rgb(121, 116, 126),
                border: egui::Color32::from_rgba_premultiplied(0, 200, 255, 45),
                primary: egui::Color32::from_rgb(pr, pg, pb),
                secondary: egui::Color32::from_rgb(41, 121, 255),
                warning: egui::Color32::from_rgb(170, 95, 0),
            }
        }
    }
}

pub(super) fn card_frame(theme: &RealtimeEguiTheme) -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .corner_radius(egui::CornerRadius::same(10))
        .fill(theme.surface)
        .stroke(egui::Stroke::new(1.0, theme.border.gamma_multiply(0.55)))
}

pub(super) fn pill_frame(theme: &RealtimeEguiTheme) -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(3))
        .corner_radius(egui::CornerRadius::same(14))
        .fill(theme.control)
        .stroke(egui::Stroke::new(1.0, theme.border.gamma_multiply(0.45)))
}

pub(super) fn compact_button(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    active: bool,
    theme: &RealtimeEguiTheme,
) -> egui::Response {
    let fill = if active {
        theme.primary.gamma_multiply(0.35)
    } else {
        theme.control
    };
    let stroke = if active {
        egui::Stroke::new(1.0, theme.primary.gamma_multiply(0.85))
    } else {
        egui::Stroke::new(1.0, theme.border.gamma_multiply(0.35))
    };
    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(label.into())
                .size(11.0)
                .color(if active { theme.text } else { theme.muted }),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(12))
        .min_size(egui::vec2(24.0, 22.0)),
    );
    if response.hovered() {
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            egui::CornerRadius::same(13),
            egui::Stroke::new(1.0, theme.control_hover),
            egui::StrokeKind::Outside,
        );
    }
    response
}

pub(super) fn render_combo(
    ui: &mut egui::Ui,
    id: &'static str,
    selected: impl Into<egui::WidgetText>,
    width: f32,
    theme: &RealtimeEguiTheme,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let response = crate::gui::widgets::combo(id)
        .selected_text(selected)
        .width(width)
        .height(240.0)
        .show_ui(ui, add_contents)
        .response;
    ui.painter().rect_stroke(
        response.rect,
        egui::CornerRadius::same(14),
        egui::Stroke::new(1.0, theme.border.gamma_multiply(0.35)),
        egui::StrokeKind::Inside,
    );
    response
}

pub(super) fn split_panel_frame(
    ui: &mut egui::Ui,
    theme: &RealtimeEguiTheme,
    width: f32,
    min_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let frame = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(8, 7))
        .corner_radius(egui::CornerRadius::same(10))
        .fill(theme.surface.gamma_multiply(0.78))
        .stroke(egui::Stroke::new(1.0, theme.border.gamma_multiply(0.3)));

    let rect = ui
        .allocate_exact_size(egui::vec2(width, min_height), egui::Sense::hover())
        .0;
    let content_rect = rect.shrink2(egui::vec2(
        f32::from(frame.inner_margin.left) + frame.stroke.width,
        f32::from(frame.inner_margin.top) + frame.stroke.width,
    ));
    ui.painter().add(frame.paint(content_rect));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    add_contents(&mut child);
}
