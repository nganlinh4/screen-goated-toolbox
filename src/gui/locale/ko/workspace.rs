use crate::gui::locale::WorkspaceLocaleText;

pub(super) fn get() -> WorkspaceLocaleText {
    WorkspaceLocaleText {
        history_btn: "히스토리",
        history_title: "결과 라이브러리",
        max_items_label: "저장 한도:",
        cc_memory_max_label: "CC 기억:",
        history_empty: "기록이 없습니다.",
        clear_all_history_btn: "모두 삭제",
        view_image_btn: "이미지 보기",
        listen_audio_btn: "오디오 듣기",
        view_text_btn: "텍스트 보기",
        tips_title: "사용 팁",
        tips_list: super::super::tips::ko(),
        tips_click_hint: "이 텍스트를 클릭하여 팁 목록 보기",
        restore_preset_btn: "복원",
        restore_preset_tooltip: "기본 설정으로 초기화",
        search_doing: "진행 중:",
        search_searching: "검색",
        search_query_label: "검색 쿼리:",
        search_found_sources: "{} 소스 발견",
        search_sources_label: "참고 소스 (관련도순):",
        search_no_title: "(제목 없음)",
        search_synthesizing: "정보 종합 중...",
        search_analyzed_sources: "{} 소스 분석 완료",
        search_processing: "결과 처리 및 요약 중...",
    }
}
