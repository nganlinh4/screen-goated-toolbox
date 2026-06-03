use crate::gui::theme::AppTheme;
use eframe::egui;

pub(super) fn render_controller_mode_description(
    ui: &mut egui::Ui,
    ui_language: &str,
    preset_type: &str,
    audio_processing_mode: &str,
) {
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

            let is_realtime = preset_type == "audio" && audio_processing_mode == "realtime";
            let title = if is_realtime {
                match ui_language {
                    "vi" => "Xử lý âm thanh (Thời gian thực)",
                    "ko" => "오디오 처리 (실시간)",
                    _ => "Audio Processing (Realtime)",
                }
            } else {
                match ui_language {
                    "vi" => "Chế độ Bộ điều khiển",
                    "ko" => "컨트롤러 모드",
                    _ => "Controller Mode",
                }
            };
            ui.label(egui::RichText::new(title).heading().color(accent_color));

            ui.add_space(16.0);

            let desc = if is_realtime {
                match ui_language {
                    "vi" => "Chế độ này cung cấp phụ đề và dịch thuật trực tiếp theo thời gian thực.\nMã API của Gemini là bắt buộc, tính năng chỉ hoạt động tốt trên âm thanh có lời nói to rõ như podcast!\n\nBạn có thể điều chỉnh cỡ chữ, nguồn âm thanh và ngôn ngữ dịch ngay trong cửa sổ kết quả.",
                    "ko" => "이 모드는 실시간 자막 및 번역을 제공합니다.\nGemini API 키가 필수이며, 명확한 음성이 있는 팟캐스트 같은 오디오에서 잘 작동합니다!\n\n결과 창에서 글꼴 크기, 오디오 소스, 번역 언어를 직접 조정할 수 있습니다.",
                    _ => "This mode provides real-time transcription and translation.\nGemini API key is required, works best on audio with clear speech like podcasts!\n\nYou can adjust font size, audio source, and translation language directly in the result window.",
                }
            } else {
                match ui_language {
                    "vi" => "Đây là cấu hình MASTER. Khi kích hoạt, một bánh xe chọn sẽ xuất hiện để bạn chọn cấu hình muốn sử dụng.\n\nChỉ cần gán một phím tắt để truy cập nhanh nhiều cấu hình khác nhau.",
                    "ko" => "이것은 MASTER 프리셋입니다. 활성화하면 프리셋 휠이 나타나 사용할 프리셋을 선택할 수 있습니다.\n\n하나의 단축키로 여러 프리셋에 빠르게 접근하세요.",
                    _ => "This is a MASTER preset. When activated, a selection wheel will appear letting you choose which preset to use.\n\nAssign a single hotkey for quick access to multiple presets.",
                }
            };
            ui.label(egui::RichText::new(desc).color(text_color));
        });
}
