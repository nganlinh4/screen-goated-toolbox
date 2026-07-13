use crate::gui::locale::WorkspaceLocaleText;

pub(super) fn get() -> WorkspaceLocaleText {
    WorkspaceLocaleText {
        history_btn: "Lịch sử",
        history_title: "Thư viện kết quả",
        max_items_label: "Giới hạn lưu:",
        cc_memory_max_label: "Bộ nhớ CC:",
        history_empty: "Chưa có lịch sử nào.",
        clear_all_history_btn: "Dọn tất cả",
        view_image_btn: "Xem ảnh",
        listen_audio_btn: "Nghe audio",
        view_text_btn: "Xem text",
        tips_title: "Mẹo sử dụng",
        tips_list: super::super::tips::vi(),
        tips_click_hint: "Click vào dòng chữ này để xem danh sách mẹo",
        restore_preset_btn: "Khôi phục",
        restore_preset_tooltip: "Đặt lại cài đặt về mặc định",
        search_doing: "Đang thực thi",
        search_searching: "tìm kiếm",
        search_query_label: "Truy vấn tìm kiếm:",
        search_found_sources: "ĐÃ TÌM THẤY {} NGUỒN",
        search_sources_label: "Nguồn tham khảo (theo độ liên quan):",
        search_no_title: "(Không có tiêu đề)",
        search_synthesizing: "ĐANG TỔNG HỢP THÔNG TIN...",
        search_analyzed_sources: "Đã phân tích {} nguồn",
        search_processing: "Đang xử lý và tóm tắt kết quả...",
    }
}
