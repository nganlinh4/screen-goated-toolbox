use crate::gui::locale::AuxiliaryLocaleText;

pub(super) fn get() -> AuxiliaryLocaleText {
    AuxiliaryLocaleText {
        download: super::download::get(),
        managed_tools: super::managed_tools::get(),
        continuous_mode_activated: "Cấu hình \"{preset}\" sẽ hoạt động liên tục, bấm ESC hay {hotkey} để thoát",
        win_select_title: "Chọn Cửa Sổ để Quay",
        win_select_subtitle: "Nhấn Escape hoặc click bên ngoài để hủy",
        win_select_count: "{} cửa sổ",
    }
}
