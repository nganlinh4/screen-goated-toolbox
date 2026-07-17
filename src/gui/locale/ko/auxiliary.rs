use crate::gui::locale::AuxiliaryLocaleText;

pub(super) fn get() -> AuxiliaryLocaleText {
    AuxiliaryLocaleText {
        download: super::download::get(),
        managed_tools: super::managed_tools::get(),
        continuous_mode_activated: "프리셋 \"{preset}\"이(가) 연속 모드로 실행됩니다. ESC 또는 {hotkey}를 눌러 종료",
        win_select_title: "녹화할 창 선택",
        win_select_subtitle: "Escape 또는 바깥 클릭으로 취소",
        win_select_count: "창 {}개",
        win_select_display_only_badge: "화면만",
        win_select_display_only_title: "화면 캡처를 사용하세요",
        win_select_display_only_message: "이 전체 화면 또는 프레젠테이션 창은 개별 창으로 안정적으로 녹화할 수 없습니다. 대신 화면 캡처를 선택하세요.",
        win_select_display_only_action: "창 목록으로 돌아가기",
    }
}
