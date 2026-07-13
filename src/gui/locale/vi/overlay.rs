use crate::gui::locale::OverlayLocaleText;

pub(super) fn get() -> OverlayLocaleText {
    OverlayLocaleText {
        overlay_copy_tooltip: "Sao chép",
        overlay_undo_tooltip: "Hoàn tác",
        overlay_redo_tooltip: "Làm lại",
        overlay_edit_tooltip: "Chỉnh sửa / Viết lại",
        overlay_refine_placeholder: "Chỉnh sửa kết quả...",
        overlay_markdown_tooltip: "Bật/Tắt Markdown",
        overlay_download_tooltip: "Tải về HTML",
        overlay_speaker_tooltip: "Đọc to (TTS)",
        overlay_broom_tooltip: "Chổi: Trái - Đóng | Phải - Đóng nhóm | Giữa - Đóng hết | Kéo - Dời | Kéo phải - Dời nhóm | Kéo giữa - Dời hết",
        overlay_back_tooltip: "Quay lại",
        overlay_forward_tooltip: "Tiếp theo",
        overlay_opacity_tooltip: "Độ mờ",
        overlay_cancel_tooltip: "Hủy",
        text_input_close_tooltip: "Đóng",
        text_input_speech_to_text_tooltip: "Giọng nói sang văn bản",
        text_input_send_tooltip: "Gửi",
        preset_wheel_cancel: "HỦY",
        select_region_badge: "Chọn vùng MH",
        unlimited_label: "∞ Không giới hạn",
        history_clear_search_tooltip: "Xóa tìm kiếm",
        history_open_media_folder_tooltip: "Mở thư mục media",
        history_delete_tooltip: "Xóa",
        history_copy_text_tooltip: "Sao chép văn bản",
    }
}
