use crate::gui::theme::AppTheme;
use eframe::egui;

pub(super) fn render_controller_mode_description(ui: &mut egui::Ui, ui_language: &str) {
    ui.add_space(20.0);

    let is_dark = ui.visuals().dark_mode;
    let theme = AppTheme::from_dark(is_dark);
    let bg_color = theme.controller_mode_bg();
    let text_color = if is_dark {
        egui::Color32::from_gray(200)
    } else {
        egui::Color32::from_gray(60)
    };
    let accent_color = theme.controller_mode_accent();

    egui::Frame::new()
        .fill(bg_color)
        .stroke(theme.card_stroke())
        .inner_margin(24.0)
        .corner_radius(12.0)
        .show(ui, |ui| {
            ui.set_min_height(260.0);

            let title = match ui_language {
                "vi" => "Chế độ Bộ điều khiển",
                "ko" => "컨트롤러 모드",
                _ => "Controller Mode",
            };
            ui.label(egui::RichText::new(title).heading().color(accent_color));

            ui.add_space(16.0);

            let desc = match ui_language {
                "vi" => "Đây là cấu hình MASTER. Khi kích hoạt, một bánh xe chọn sẽ xuất hiện để bạn chọn cấu hình muốn sử dụng.\n\nChỉ cần gán một phím tắt để truy cập nhanh nhiều cấu hình khác nhau.",
                "ko" => "이것은 MASTER 프리셋입니다. 활성화하면 프리셋 휠이 나타나 사용할 프리셋을 선택할 수 있습니다.\n\n하나의 단축키로 여러 프리셋에 빠르게 접근하세요.",
                _ => "This is a MASTER preset. When activated, a selection wheel will appear letting you choose which preset to use.\n\nAssign a single hotkey for quick access to multiple presets.",
            };
            ui.label(egui::RichText::new(desc).color(text_color));
        });
}
