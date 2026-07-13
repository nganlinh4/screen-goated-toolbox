use crate::gui::locale::ModelCatalogLocaleText;

pub(super) fn get() -> ModelCatalogLocaleText {
    ModelCatalogLocaleText {
        model_priority_button: "Ưu tiên mô hình",
        model_priority_title: "Chuỗi ưu tiên mô hình",
        model_priority_image_chain_title: "Ảnh → Text",
        model_priority_text_chain_title: "Text → Text",
        model_priority_chosen_model: "Mô hình đã chọn",
        model_priority_fixed_hint: "luôn thử đầu tiên",
        model_priority_add_model: "+ Thêm mô hình",
        model_priority_auto: "Tự động",
        model_priority_auto_hint: "tiếp tục theo thứ tự fallback thông minh",
        model_priority_skip_hint: "Nhà cung cấp tắt, thiếu khóa, khóa sai, hoặc mô hình không hỗ trợ sẽ bị bỏ qua ngay khi thử lại.",
        custom_models_button: "Tuỳ chỉnh mô hình",
        custom_models_title: "Tuỳ chỉnh mô hình",
        custom_models_desc: "Quản lý mô hình tự thêm. Mô hình mặc định chỉ xem, không chỉnh sửa.",
        custom_models_builtin_locked: "Mặc định - đã khóa",
        custom_models_discovered_models: "Tự quét",
        custom_models_add_openrouter: "+ Thêm OpenRouter",
        custom_models_import_openrouter: "Quét OpenRouter",
        custom_models_scan_ollama: "Quét Ollama",
        custom_models_no_models: "Chưa có mô hình",
        custom_models_display_name: "Tên hiển thị",
        custom_models_api_model: "Tên API",
        custom_models_type: "Loại",
        custom_models_text_type: "Văn bản",
        custom_models_vision_type: "Hình ảnh",
        custom_models_search: "Tìm kiếm",
        custom_models_enabled: "Bật",
    }
}
