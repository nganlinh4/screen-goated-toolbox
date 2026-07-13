use crate::gui::locale::OverlayLocaleText;

pub(super) fn get() -> OverlayLocaleText {
    OverlayLocaleText {
        overlay_copy_tooltip: "복사",
        overlay_undo_tooltip: "실행 취소",
        overlay_redo_tooltip: "다시 실행",
        overlay_edit_tooltip: "편집 / 다듬기",
        overlay_refine_placeholder: "결과 수정...",
        overlay_markdown_tooltip: "마크다운 토글",
        overlay_download_tooltip: "HTML 저장",
        overlay_speaker_tooltip: "텍스트 읽기 (TTS)",
        overlay_broom_tooltip: "빗자루: 왼쪽 - 닫기 | 오른쪽 - 그룹 닫기 | 가운데 - 모두 닫기 | 드래그 - 이동 | 오른쪽 드래그 - 그룹 이동 | 가운데 드래그 - 모두 이동",
        overlay_back_tooltip: "뒤로",
        overlay_forward_tooltip: "앞으로",
        overlay_opacity_tooltip: "불투명도",
        overlay_cancel_tooltip: "취소",
        text_input_close_tooltip: "닫기",
        text_input_speech_to_text_tooltip: "음성을 텍스트로",
        text_input_send_tooltip: "보내기",
        preset_wheel_cancel: "취소",
        select_region_badge: "화면 영역 선택",
        unlimited_label: "∞ 무제한",
        history_clear_search_tooltip: "검색 지우기",
        history_open_media_folder_tooltip: "미디어 폴더 열기",
        history_delete_tooltip: "삭제",
        history_copy_text_tooltip: "텍스트 복사",
    }
}
