use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Dịch MH (Circle to Search)",
        screen_translate_short_btn: "Dịch MH",
        screen_translate_title: "Dịch MH (Circle to Search)",
        screen_translate_intro: "Khoanh một vùng màn hình. SGT tìm, dịch và phủ từng dòng chữ ngay tại chỗ.",
        screen_translate_target_label: "Dịch sang",
        screen_translate_locator_label: "Định vị chữ",
        screen_translate_locator_model: "PaddleOCR đa ngôn ngữ",
        screen_translate_fixed_badge: "Cố định",
        screen_translate_model_label: "Mô hình dịch",
        screen_translate_model_fallback_hint: "Nếu lỗi, thử tiếp danh sách ưu tiên Văn bản → Văn bản.",
        screen_translate_prompt_label: "Lệnh dịch",
        screen_translate_prompt_hint: "Dùng {target_language} cho ngôn ngữ đích đã chọn.",
        screen_translate_opacity_label: "Độ mờ lớp phủ",
        screen_translate_hotkey_label: "Phím tắt",
        screen_translate_hotkey_empty: "Chưa đặt phím tắt",
        screen_translate_locating: "Đang tìm và dịch chữ…",
        screen_translate_no_text: "Không tìm thấy chữ cần dịch",
        screen_translate_error: "Dịch thất bại",
    }
}
