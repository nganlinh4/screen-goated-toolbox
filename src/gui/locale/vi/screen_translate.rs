use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Dịch màn hình",
        screen_translate_title: "Dịch màn hình",
        screen_translate_intro: "Khoanh một vùng trên màn hình. SGT tìm và dịch mọi dòng chữ rồi phủ bản dịch đúng vị trí.",
        screen_translate_target_label: "Dịch sang",
        screen_translate_hotkey_label: "Phím tắt",
        screen_translate_hotkey_empty: "Chưa đặt phím tắt",
        screen_translate_locating: "Đang tìm và dịch chữ…",
        screen_translate_no_text: "Không tìm thấy chữ cần dịch",
        screen_translate_error: "Dịch thất bại",
    }
}
