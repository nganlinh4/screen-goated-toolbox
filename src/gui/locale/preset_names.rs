//! Display names of the built-in presets in every UI language.
//!
//! One row per preset is the single source for these names. Each row names
//! every UI language, so adding a language does not compile until every
//! built-in preset has its name in it.

use super::UiLanguage;

struct PresetName {
    id: &'static str,
    en: &'static str,
    ko: &'static str,
    vi: &'static str,
}

impl PresetName {
    fn in_language(&self, language: UiLanguage) -> &'static str {
        match language {
            UiLanguage::En => self.en,
            UiLanguage::Ko => self.ko,
            UiLanguage::Vi => self.vi,
        }
    }
}

/// The name a preset shows in the UI: its translation when it is built in,
/// otherwise its id made readable.
pub fn get_localized_preset_name(preset_id: &str, lang_code: &str) -> String {
    match PRESET_NAMES.iter().find(|name| name.id == preset_id) {
        Some(name) => name
            .in_language(UiLanguage::from_code(lang_code))
            .to_string(),
        None => preset_id
            .strip_prefix("preset_")
            .unwrap_or(preset_id)
            .replace('_', " "),
    }
}

const PRESET_NAMES: &[PresetName] = &[
    PresetName {
        id: "preset_101_on_this",
        en: "101 on this",
        ko: "이것의 모든 것",
        vi: "Tất tần tật",
    },
    PresetName {
        id: "preset_translate",
        en: "Translate region",
        ko: "영역 번역",
        vi: "Dịch vùng",
    },
    PresetName {
        id: "preset_translate_auto_paste",
        en: "Trans reg (Auto paste)",
        ko: "영역 번역 (자동 붙.)",
        vi: "Dịch vùng (Tự dán)",
    },
    PresetName {
        id: "preset_translate_retranslate",
        en: "Trans reg+Retrans",
        ko: "영역 번역+재번역",
        vi: "Dịch vùng+Dịch lại",
    },
    PresetName {
        id: "preset_ocr",
        en: "Extract text",
        ko: "텍스트 추출",
        vi: "Lấy text từ ảnh",
    },
    PresetName {
        id: "preset_quick_screenshot",
        en: "Quick screenshot",
        ko: "빠른 스크린샷",
        vi: "Chụp MH nhanh",
    },
    PresetName {
        id: "preset_ocr_read",
        en: "Read this region",
        ko: "영역 읽기",
        vi: "Đọc vùng này",
    },
    PresetName {
        id: "preset_summarize",
        en: "Summarize region",
        ko: "영역 요약",
        vi: "Tóm tắt vùng",
    },
    PresetName {
        id: "preset_desc",
        en: "Describe image",
        ko: "이미지 설명",
        vi: "Mô tả ảnh",
    },
    PresetName {
        id: "preset_ask_image",
        en: "Ask about image",
        ko: "이미지 질문",
        vi: "Hỏi về ảnh",
    },
    PresetName {
        id: "preset_translate_select",
        en: "Trans (Select text)",
        ko: "번역 (선택 텍스트)",
        vi: "Dịch",
    },
    PresetName {
        id: "preset_translate_arena",
        en: "Trans (Arena)",
        ko: "번역 (아레나)",
        vi: "Dịch (Arena)",
    },
    PresetName {
        id: "preset_read_aloud",
        en: "Read aloud",
        ko: "크게 읽기",
        vi: "Đọc to",
    },
    PresetName {
        id: "preset_trans_retrans_select",
        en: "Trans+Retrans (Select)",
        ko: "번역+재번역 (선택)",
        vi: "Dịch+ Dịch lại",
    },
    PresetName {
        id: "preset_select_translate_replace",
        en: "Select-Trans-Replace",
        ko: "선택-번역-교체",
        vi: "Dịch và Thay",
    },
    PresetName {
        id: "preset_fix_grammar",
        en: "Fix Grammar",
        ko: "문법 수정",
        vi: "Sửa ngữ pháp",
    },
    PresetName {
        id: "preset_rephrase",
        en: "Rephrase",
        ko: "다시 쓰기",
        vi: "Viết lại",
    },
    PresetName {
        id: "preset_make_formal",
        en: "Make Formal",
        ko: "공식적으로",
        vi: "Chuyên nghiệp hóa",
    },
    PresetName {
        id: "preset_explain",
        en: "Explain",
        ko: "설명",
        vi: "Giải thích",
    },
    PresetName {
        id: "preset_ask_text",
        en: "Ask about text...",
        ko: "텍스트 질문...",
        vi: "Hỏi về text...",
    },
    PresetName {
        id: "preset_edit_as_follows",
        en: "Edit as follows:",
        ko: "다음과 같이 수정:",
        vi: "Sửa như sau:",
    },
    PresetName {
        id: "preset_extract_table",
        en: "Extract Table",
        ko: "표 추출",
        vi: "Trích bảng",
    },
    PresetName {
        id: "preset_qr_scanner",
        en: "QR Scanner",
        ko: "QR 스캔",
        vi: "Quét mã QR",
    },
    PresetName {
        id: "preset_trans_retrans_typing",
        en: "Trans+Retrans (Type)",
        ko: "번역+재번역 (입력)",
        vi: "Dịch+Dịch lại (Tự gõ)",
    },
    PresetName {
        id: "preset_ask_ai",
        en: "Ask AI",
        ko: "AI 질문",
        vi: "Hỏi AI",
    },
    PresetName {
        id: "preset_internet_search",
        en: "Internet Search",
        ko: "인터넷 검색",
        vi: "Tìm kiếm internet",
    },
    PresetName {
        id: "preset_make_game",
        en: "Make a Game",
        ko: "게임 만들기",
        vi: "Tạo con game",
    },
    PresetName {
        id: "preset_transcribe",
        en: "Transcribe speech",
        ko: "음성 받아쓰기",
        vi: "Lời nói thành văn",
    },
    PresetName {
        id: "preset_fix_pronunciation",
        en: "Fix pronunciation",
        ko: "발음 교정",
        vi: "Chỉnh phát âm",
    },
    PresetName {
        id: "preset_study_language",
        en: "Study language",
        ko: "언어 학습",
        vi: "Học ngoại ngữ",
    },
    PresetName {
        id: "preset_transcribe_retranslate",
        en: "Quick 4NR reply 1",
        ko: "빠른 외국인 답변 1",
        vi: "Trả lời ng.nc.ngoài 1",
    },
    PresetName {
        id: "preset_quicker_foreigner_reply",
        en: "Quick 4NR reply 2",
        ko: "빠른 외국인 답변 2",
        vi: "Trả lời ng.nc.ngoài 2",
    },
    PresetName {
        id: "preset_fact_check",
        en: "Fact Check",
        ko: "정보 확인",
        vi: "Kiểm chứng thông tin",
    },
    PresetName {
        id: "preset_omniscient_god",
        en: "Omniscient God",
        ko: "전지전능한 신",
        vi: "Thần Trí tuệ",
    },
    PresetName {
        id: "preset_quick_ai_question",
        en: "Quick AI Question",
        ko: "빠른 AI 질문",
        vi: "Hỏi nhanh AI",
    },
    PresetName {
        id: "preset_voice_search",
        en: "Voice Search",
        ko: "음성 검색",
        vi: "Nói để search",
    },
    PresetName {
        id: "preset_hang_image",
        en: "Image Overlay",
        ko: "이미지 오버레이",
        vi: "Treo ảnh",
    },
    PresetName {
        id: "preset_hang_text",
        en: "Text Overlay",
        ko: "텍스트 오버레이",
        vi: "Treo text",
    },
    PresetName {
        id: "preset_quick_note",
        en: "Quick Note",
        ko: "빠른 메모",
        vi: "Note nhanh",
    },
    PresetName {
        id: "preset_quick_record",
        en: "Quick Record",
        ko: "빠른 녹음",
        vi: "Thu âm nhanh",
    },
    PresetName {
        id: "preset_record_device",
        en: "Device Record",
        ko: "시스템 녹음",
        vi: "Thu âm máy",
    },
    PresetName {
        id: "preset_continuous_writing_online",
        en: "Continuous Writing",
        ko: "연속 입력",
        vi: "Viết liên tục",
    },
    PresetName {
        id: "preset_transcribe_english_offline",
        en: "Continuous transcription",
        ko: "연속 받아쓰기",
        vi: "Chép liên tục",
    },
    PresetName {
        id: "preset_image_master",
        en: "Image MASTER",
        ko: "이미지 마스터",
        vi: "Ảnh MASTER",
    },
    PresetName {
        id: "preset_text_select_master",
        en: "Select MASTER",
        ko: "선택 마스터",
        vi: "Bôi MASTER",
    },
    PresetName {
        id: "preset_text_type_master",
        en: "Type MASTER",
        ko: "입력 마스터",
        vi: "Gõ MASTER",
    },
    PresetName {
        id: "preset_audio_mic_master",
        en: "Mic MASTER",
        ko: "마이크 마스터",
        vi: "Mic MASTER",
    },
    PresetName {
        id: "preset_audio_device_master",
        en: "Sound MASTER",
        ko: "사운드 마스터",
        vi: "Tiếng MASTER",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_preset_has_a_name_in_every_ui_language() {
        for preset in crate::config::preset::defaults::get_default_presets() {
            let row = PRESET_NAMES
                .iter()
                .find(|name| name.id == preset.id)
                .unwrap_or_else(|| panic!("{} has no localized name", preset.id));
            for name in [row.en, row.ko, row.vi] {
                assert!(!name.trim().is_empty(), "{}", preset.id);
            }
        }
    }

    #[test]
    fn preset_ids_are_unique_and_names_resolve_like_locale_text() {
        let mut ids: Vec<_> = PRESET_NAMES.iter().map(|name| name.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESET_NAMES.len());

        assert_eq!(
            get_localized_preset_name("preset_translate", "en"),
            "Translate region"
        );
        assert_eq!(
            get_localized_preset_name("preset_translate", "ko"),
            "영역 번역"
        );
        assert_eq!(
            get_localized_preset_name("preset_translate", "vi"),
            "Dịch vùng"
        );
        // Unknown UI languages fall back to English, as LocaleText does.
        assert_eq!(
            get_localized_preset_name("preset_translate", "fr"),
            "Translate region"
        );
        assert_eq!(
            get_localized_preset_name("preset_101_on_this", "ko"),
            "이것의 모든 것"
        );
        assert_eq!(
            get_localized_preset_name("preset_my_own_thing", "vi"),
            "my own thing"
        );
        assert_eq!(get_localized_preset_name("user-42", "ko"), "user-42");
    }
}
