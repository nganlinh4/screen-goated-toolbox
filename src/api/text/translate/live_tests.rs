//! Live check that a model obeys the plain translate instruction.
//!
//! Not a unit test: it calls real endpoints and is ignored by default. Whether a
//! translation that adds content is the model's fault or the request's can only
//! be settled by sending the same prompt to many models and reading what comes
//! back.
//!
//!     $env:CATALOG_BENCH_LIVE = "1"
//!     $env:TRANSLATE_ADHERENCE_SAMPLES = "3"
//!     $env:TRANSLATE_ADHERENCE_LANGUAGE = "English"
//!     cargo test --bin screen-goated-toolbox translate_adherence -- --ignored --nocapture
//!
//! `CATALOG_BENCH_MODELS` narrows it to named models, as elsewhere in the
//! benchmark.

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::catalog_benchmark::setup::Credentials;
use crate::model_config::ModelType;
use std::time::Duration;

/// Target language name, as the preset picker shows it; any catalog language works.
fn target_language() -> String {
    std::env::var("TRANSLATE_ADHERENCE_LANGUAGE").unwrap_or_else(|_| "English".to_string())
}

/// The preset's prompt, verbatim, with `{language1}` resolved.
fn instruction(language: &str) -> String {
    format!("Translate the following text to {language}. Output ONLY the translation.")
}

/// Synthetic workplace notes in the shape that invites invented content: an
/// opening address to the reader, figures, and embedded Latin-script terms.
const CASES: &[(&str, &str)] = &[
    (
        "addressed-business-note",
        "팀장님, 요청하신 주간 보고서에 지역별 매출과 담당자 컬럼을 추가했습니다. 수치는 분기별로 구분해 표시했으며 Excel 파일에도 같은 내용을 포함했습니다. 첨부 화면은 테스트 환경에서 캡처한 것으로 실제 값과 다릅니다. 확인 부탁드립니다.",
    ),
    (
        "addressed-note-with-latin-terms",
        "팀장님, 점수는 동일한 baseline 12개 지표의 Z-score를 각 모델로 계산한 뒤 공통 0.5 기준에 맞춰 표준화한 0~1 지표입니다. 기존 모델은 0.20, 0.50, 0.80 구간값을, 신규 모델은 0.15, 0.45, 0.70 구간값을 사용해 등급을 표시합니다.",
    ),
];

/// Bracketed template slots in the reply that the source does not contain,
/// such as a signature placeholder: the model completing a template it made up
/// rather than translating.
fn invented_placeholders(source: &str, reply: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = reply;
    while let Some(start) = rest.find('[') {
        let Some(length) = rest[start..].find(']') else {
            break;
        };
        let slot = &rest[start..=start + length];
        if !source.contains(slot) && !found.iter().any(|known| known == slot) {
            found.push(slot.to_string());
        }
        rest = &rest[start + length + 1..];
    }
    found
}

#[test]
#[ignore = "calls live provider endpoints"]
fn translate_adherence_across_models() {
    if std::env::var("CATALOG_BENCH_LIVE").as_deref() != Ok("1") {
        eprintln!("set CATALOG_BENCH_LIVE=1 to run");
        return;
    }
    let credentials = Credentials::load().expect("load benchmark credentials");
    let filter = crate::catalog_benchmark::setup::model_filter();
    let models = crate::catalog_benchmark::setup::select_models(
        ModelType::Text,
        filter.as_ref(),
        None,
        &credentials,
    );
    assert!(!models.is_empty(), "no text model has credentials");

    // One sample cannot separate a model that always does this from one that did
    // it once, and only a repeating fault is worth acting on.
    let samples: usize = std::env::var("TRANSLATE_ADHERENCE_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);

    let language = target_language();
    let expected = crate::config::language_from_name(&language)
        .unwrap_or_else(|| panic!("unknown target language {language}"))
        .to_639_3();
    let instruction = instruction(&language);

    let mut rows: Vec<(String, String, String)> = Vec::new();
    for (case_id, source) in CASES {
        for model in &models {
            for sample in 0..samples {
                let result = credentials.with_provider_key(&model.provider, |key| {
                    translate_text_streaming(
                        TranslateTextRequest {
                            max_output_tokens: None,
                            search_enabled: false,
                            search_capability: None,
                            search_tool_enabled_by_default: None,
                            groq_api_key: Credentials::groq_key_for(&model.provider, key),
                            gemini_api_key: key,
                            text: source.to_string(),
                            instruction: instruction.clone(),
                            model: model.full_name.clone(),
                            provider: model.provider.clone(),
                            streaming_enabled: true,
                            use_json_format: false,
                            response_schema: None,
                            search_label: None,
                            ui_language: "en",
                            cancel_token: None,
                            request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                                Duration::from_secs(60),
                            )),
                            target_language: Some(language.clone()),
                        },
                        |_| {},
                    )
                });
                let (verdict, detail) = match &result {
                    Err(error) => ("ERROR".to_string(), error.to_string()),
                    Ok(reply) => {
                        let detected = crate::lang_detect::detect_language(reply);
                        let placeholders = invented_placeholders(source, reply);
                        if detected.as_deref() != Some(expected) {
                            (
                                "WRONG-LANG".to_string(),
                                format!(
                                    "{detected:?}: {}",
                                    reply.chars().take(80).collect::<String>()
                                ),
                            )
                        } else if !placeholders.is_empty() {
                            ("TEMPLATE".to_string(), placeholders.join(", "))
                        } else {
                            ("ok".to_string(), format!("{} chars", reply.chars().count()))
                        }
                    }
                };
                println!(
                    "{case_id:<24} {:<46} #{sample} {verdict:<9} {detail}",
                    model.full_name
                );
                rows.push((case_id.to_string(), model.full_name.clone(), verdict));
            }
        }
    }

    println!("\n=== models that did not comply ===");
    for (case_id, model, verdict) in &rows {
        if verdict != "ok" && verdict != "ERROR" {
            println!("{case_id:<24} {model:<46} {verdict}");
        }
    }
}

#[test]
fn placeholders_are_bracketed_slots_absent_from_the_source() {
    assert_eq!(
        invented_placeholders(
            "keep [A1] as is",
            "kept [A1]. Regards, [Name] [Name] [Title]"
        ),
        ["[Name]", "[Title]"]
    );
    assert!(invented_placeholders("no slots", "plain reply [unclosed").is_empty());
}
