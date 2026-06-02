use super::{
    CleanNarrationItem, NarrationRequestGroup, SubtitleNarrationGroupingRequest, TtsProfileWire,
    build_narration_groups, handle_get_narration_tts_metadata, normalize_group_sentence,
    normalize_narration_input_text, prepare_narration_tts_text, split_group_audio_ranges,
};
use crate::api::tts::types::TtsCollectedAudio;
use crate::config::TtsMethod;

#[test]
fn step_audio_profile_wire_preserves_reference_id_and_prompt() {
    let wire: TtsProfileWire = serde_json::from_value(serde_json::json!({
        "method": "StepAudioEditX",
        "stepAudioVoice": "",
        "stepAudioReferenceVoiceId": "ref-demo",
        "stepAudioPromptText": "Use a calm narration delivery."
    }))
    .expect("deserialize Step Audio narration profile");

    let profile = wire.into_request_profile(Some("cmn".to_string()));

    assert_eq!(profile.method, TtsMethod::StepAudioEditX);
    assert_eq!(profile.language_code_override.as_deref(), Some("cmn"));
    assert_eq!(
        profile.step_audio_settings.style_prompt,
        "Use a calm narration delivery."
    );
    assert_eq!(profile.step_audio_settings.reference_voice_id, "ref-demo");
}

#[test]
fn narration_tts_metadata_exposes_step_audio_options() {
    let metadata = handle_get_narration_tts_metadata(&serde_json::Value::Null)
        .expect("get narration TTS metadata");

    assert!(
        metadata["providers"]
            .as_array()
            .expect("providers array")
            .iter()
            .any(|provider| provider["method"] == "StepAudioEditX")
    );
    assert!(metadata["stepAudioReferenceVoices"].is_array());
    assert!(metadata["defaults"]["stepAudioReferenceVoiceId"].is_string());
}

#[test]
fn magpie_narration_adds_terminal_punctuation_to_fragments() {
    assert_eq!(
        prepare_narration_tts_text("Đêm giông bão", &TtsMethod::MagpieMultilingual),
        "Đêm giông bão."
    );
    assert_eq!(
        prepare_narration_tts_text("Đêm giông bão!", &TtsMethod::MagpieMultilingual),
        "Đêm giông bão!"
    );
    assert_eq!(
        prepare_narration_tts_text("Đêm giông bão", &TtsMethod::GeminiLive),
        "Đêm giông bão"
    );
}

#[test]
fn narration_normalization_repairs_cp949_mojibake() {
    assert_eq!(
        normalize_narration_input_text("휂챗m gi척ng b찾o", &TtsMethod::VieneuTts).as_deref(),
        Some("Đêm giông bão")
    );
    assert_eq!(
        normalize_narration_input_text("??R梳캮G CH횣NG T횚I C횙 CH梳짽??", &TtsMethod::VieneuTts)
            .as_deref(),
        Some("RẰNG CHÚNG TÔI CÓ CHẤT")
    );
}

#[test]
fn narration_normalization_skips_unrecoverable_mojibake_placeholders() {
    assert_eq!(
        normalize_narration_input_text(
            "??V? NH梳줪 THEO NH沼둗 휂I沼괣- ??",
            &TtsMethod::VieneuTts
        ),
        None
    );
    assert_eq!(
        normalize_narration_input_text("[V?O 휂칩A]", &TtsMethod::VieneuTts),
        None
    );
}

#[test]
fn narration_normalization_skips_unspeakable_fragments() {
    assert_eq!(
        normalize_narration_input_text("????", &TtsMethod::VieneuTts),
        None
    );
}

#[test]
fn narration_normalization_strips_music_wrappers_per_line() {
    assert_eq!(
        normalize_narration_input_text(
            "♪♪TÔI VÀ CÔ GÁI CỦA TÔI♪♪\n♪♪MỐI QUAN HỆ NÀY♪♪",
            &TtsMethod::VieneuTts
        )
        .as_deref(),
        Some("TÔI VÀ CÔ GÁI CỦA TÔI\nMỐI QUAN HỆ NÀY")
    );
}

fn clean_item(id: &str, text: &str, start_time: f64, end_time: f64) -> CleanNarrationItem {
    CleanNarrationItem {
        id: id.to_string(),
        text: text.to_string(),
        tts_text: text.to_string(),
        aligner_text: super::normalize_alignment_text(text),
        start_time,
        end_time,
        text_units: super::estimate_narration_speech_units(text),
    }
}

#[test]
fn narration_grouping_respects_text_budget_and_timing_gaps() {
    let grouping = SubtitleNarrationGroupingRequest {
        text_budget_units: 4,
        vad_search_radius_sec: 0.35,
    };
    let groups = build_narration_groups(
        vec![
            clean_item("a", "one two", 0.0, 0.5),
            clean_item("b", "three four", 0.6, 1.0),
            clean_item("c", "five", 3.0, 3.4),
        ],
        &grouping,
    );

    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0]
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(groups[1].items[0].id, "c");
}

#[test]
fn narration_group_sentence_join_adds_periods_for_vad_pauses() {
    assert_eq!(
        normalize_group_sentence("♪ TÔI VÀ CÔ GÁI ♪"),
        "TÔI VÀ CÔ GÁI."
    );
    assert_eq!(normalize_group_sentence("Bạn ổn không?"), "Bạn ổn không?");
    assert_eq!(normalize_group_sentence("Chạy nào,"), "Chạy nào.");
}

#[test]
fn narration_group_split_ranges_are_monotonic() {
    let group = NarrationRequestGroup {
        id: "group-0".to_string(),
        items: vec![
            clean_item("a", "xin chào", 0.0, 0.6),
            clean_item("b", "tạm biệt mọi người", 0.6, 1.4),
        ],
        text: "xin chào. tạm biệt mọi người.".to_string(),
        spans: Vec::new(),
    };
    let audio = TtsCollectedAudio {
        pcm_samples: vec![0; 24_000],
        wav_data: Vec::new(),
        sample_rate: 24_000,
        duration_ms: 1000,
    };
    let split = split_group_audio_ranges(&group, &audio, 0.35);
    let ranges = split.ranges;

    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].start_sec, 0.0);
    assert!(ranges[0].end_sec > ranges[0].start_sec);
    assert!(ranges[1].start_sec >= ranges[0].end_sec);
    assert_eq!(ranges[1].end_sec, 1.0);
}
