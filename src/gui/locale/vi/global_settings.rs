use crate::gui::locale::GlobalSettingsLocaleText;

pub(super) fn get() -> GlobalSettingsLocaleText {
    GlobalSettingsLocaleText {
        controller_checkbox_label: "Bộ điều khiển",
        api_keys_header: "Mã API",
        groq_label: "Mã API Groq:",
        software_update_header: "Cập Nhật Phần Mềm",
        donate_header: "Ủng hộ tác giả",
        donate_body: "SGT là phần mềm miễn phí. Nếu thấy hữu ích, bạn có thể ủng hộ tác giả qua chuyển khoản ngân hàng (VietQR). Cảm ơn bạn rất nhiều!",
        donate_note: "",
        donate_vietnamese: true,
        startup_display_header: "Khởi Động & Hiển Thị",
        favorite_overlay_opacity_label: "Độ mờ mặc định cửa sổ kết quả",
        model_thinking: "Đang suy nghĩ...",
        ollama_url_guide: "Xem hướng dẫn tại ollama.com",
    }
}
