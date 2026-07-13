use crate::gui::locale::AuxiliaryLocaleText;

pub(super) fn get() -> AuxiliaryLocaleText {
    AuxiliaryLocaleText {
        download: super::download::get(),
        managed_tools: super::managed_tools::get(),
        continuous_mode_activated: "프리셋 \"{preset}\"이(가) 연속 모드로 실행됩니다. ESC 또는 {hotkey}를 눌러 종료",
        win_select_title: "녹화할 창 선택",
        win_select_subtitle: "Escape 또는 바깥 클릭으로 취소",
        win_select_count: "창 {}개",
    }
}
