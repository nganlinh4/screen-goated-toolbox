use crate::gui::locale::ScreenTranslateLocaleText;

pub(super) fn get() -> ScreenTranslateLocaleText {
    ScreenTranslateLocaleText {
        screen_translate_btn: "Dịch màn hình",
        screen_translate_short_btn: "Dịch MH",
        screen_translate_title: "Dịch màn hình",
        screen_translate_intro: "Vẽ hộp hoặc dùng vùng đã lưu để dịch chữ ngay tại chỗ.",
        screen_translate_target_label: "Dịch sang",
        screen_translate_recognition_label: "Nhận diện · PP-OCRv6 + PP-DocLayoutV3",
        screen_translate_recognition_hint: "Đọc chữ đa ngôn ngữ và dựa vào bố cục để nhóm các dòng liên quan trước khi dịch.",
        screen_translate_setup_hint: "Thành phần nhận diện được tải xuống khi cần và nạp khi dùng tính năng này. Tiến độ được hiển thị trên màn hình.",
        screen_translate_restore_label: "Đặt lại cài đặt",
        screen_translate_restore_hint: "Đặt lại ngôn ngữ, mô hình, hướng dẫn dịch, độ mờ và vùng chụp. Giữ nguyên cả hai nhóm phím tắt.",
        screen_translate_model_label: "Mô hình dịch",
        screen_translate_model_fallback_hint: "Nếu mô hình đã chọn không khả dụng hoặc gặp lỗi, SGT dùng danh sách ưu tiên Văn bản → Văn bản của bạn.",
        screen_translate_prompt_label: "Điều chỉnh prompt dịch nếu muốn",
        screen_translate_prompt_hint: "Tùy chỉnh giọng văn hoặc thuật ngữ. {target_language} là ngôn ngữ đã chọn; việc căn chữ được xử lý tự động.",
        screen_translate_opacity_label: "Độ mờ mặc định của lớp phủ",
        screen_translate_opacity_hint: "Áp dụng cho bản dịch mới. Khi đang xem, dùng điều khiển trên lớp phủ để chỉnh độ mờ riêng.",
        screen_translate_hotkey_label: "Phím tắt vẽ hộp",
        screen_translate_fullscreen_hotkey_label: "Phím tắt dịch toàn MH",
        screen_translate_adjust_region: "Điều chỉnh vùng",
        screen_translate_fullscreen_hint: "Dịch ngay vùng đã lưu; mặc định là toàn màn hình dưới con trỏ. Kéo 8 điểm để chỉnh vùng, dấu ✓ để lưu hoặc Esc để hủy.",
        screen_translate_hotkey_empty: "Chưa đặt phím tắt",
        screen_translate_locating: "Đang đọc và dịch vùng đã chọn…",
        screen_translate_preparing: "Đang chuẩn bị cho lần đầu chạy...",
        screen_translate_no_text: "Không tìm thấy chữ cần dịch",
        screen_translate_error: "Dịch thất bại",
    }
}
