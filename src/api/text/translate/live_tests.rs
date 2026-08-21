//! Live check that a model obeys the plain translate instruction.
//!
//! Not a unit test: it calls real endpoints and is ignored by default. It exists
//! because two adherence failures were reported from the `Trans (Select text)`
//! preset, and "is this the model or is this us" can only be settled by sending
//! the same prompt to many models and reading what comes back.
//!
//!     $env:CATALOG_BENCH_LIVE = "1"
//!     $env:TRANSLATE_ADHERENCE_SAMPLES = "3"
//!     cargo test --bin screen-goated-toolbox translate_adherence -- --ignored --nocapture
//!
//! `CATALOG_BENCH_MODELS` narrows it to named models, as elsewhere in the
//! benchmark.

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::catalog_benchmark::setup::Credentials;
use crate::model_config::ModelType;
use std::time::Duration;

/// The preset's prompt, verbatim, with `{language1}` already resolved.
const INSTRUCTION: &str =
    "Translate the following text to Vietnamese. Output ONLY the translation.";

/// The two reported failures, as the user sent them.
const CASES: &[(&str, &str)] = &[
    (
        "korean-business-note",
        "대표님, 요청하신 검체별 결과 요약에 수검자명과 점수 컬럼을 추가했습니다. 점수는 모델별로 구분해 표시하며 Excel에도 현재 서비스 모델과 단조 XGB 모델의 점수를 각각 포함했습니다. 첨부 화면은 제 개인 테스트 환경에서 캡처한 것으로, 표시된 인원수와 수치는 운영 환경의 실제 값과 다릅니다. 확인 부탁드립니다.",
    ),
    (
        "korean-with-latin-terms",
        "대표님, 점수는 동일한 corrected 19개 마커 Z-score를 각 모델로 계산한 뒤, 모델마다 다른 고정 임계값을 공통 0.5 기준에 맞춰 표준화한 0~1 위험지표이며 암 발생확률은 아닙니다. 현재 서비스 모델은 0.25·0.50·0.75, 단조 XGB 모델은 0.10·0.47·0.75의 각 모델별 고정 구간값으로 양호·관심·주의·관리를 표시합니다. 단조 XGB의 점수는 25개 fold의 표준화 점수 평균이며, 모델 판정은 기존 합의대로 25개 fold 다수결을 유지합니다.",
    ),
];

/// Vietnamese-specific letters. Their absence in a reply that should be
/// Vietnamese is the signature of a model that answered in another language.
fn has_vietnamese_marks(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            'ă' | 'â' | 'đ' | 'ê' | 'ô' | 'ơ' | 'ư' | 'Ă' | 'Â' | 'Đ' | 'Ê' | 'Ô' | 'Ơ' | 'Ư'
        ) || matches!(c as u32, 0x0300..=0x0323)
    })
}

/// Whether the reply is plain ASCII prose, i.e. never translated at all.
fn looks_like_english(text: &str) -> bool {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return false;
    }
    letters.iter().filter(|c| c.is_ascii()).count() as f32 / letters.len() as f32 > 0.95
}

/// Template placeholders, which no source can justify.
///
/// Kept apart from the sign-off below because the two are not equally damning.
/// Both sources open with `대표님,` -- an address to the reader -- so rendering
/// that as `Kính gửi ...` is a defensible translation, not an invention. A
/// trailing `[Your Name]` translates nothing: it is the model completing an
/// email template it made up.
fn invented_placeholders(reply: &str) -> Vec<&'static str> {
    let lowered = reply.to_lowercase();
    [
        "[your name]",
        "[họ và tên]",
        "[chức vụ]",
        "[thông tin liên hệ]",
        "[position]",
        "[contact",
        "[tên",
    ]
    .into_iter()
    .filter(|marker| lowered.contains(marker))
    .collect()
}

/// A closing the source does not contain.
fn invented_signoff(reply: &str) -> bool {
    reply.to_lowercase().contains("trân trọng")
}

#[test]
#[ignore = "calls live provider endpoints"]
fn translate_adherence_across_models() {
    if std::env::var("CATALOG_BENCH_LIVE").as_deref() != Ok("1") {
        eprintln!("set CATALOG_BENCH_LIVE=1 to run");
        return;
    }
    let credentials = Credentials::load();
    let filter = crate::catalog_benchmark::setup::model_filter();
    let models = crate::catalog_benchmark::setup::select_models(
        ModelType::Text,
        filter.as_ref(),
        &credentials,
    );
    assert!(!models.is_empty(), "no text model has credentials");

    // One sample cannot separate a model that always does this from one that did
    // it once, and only a repeating fault is worth acting on.
    let samples: usize = std::env::var("TRANSLATE_ADHERENCE_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);

    let mut rows: Vec<(String, String, String)> = Vec::new();
    for (case_id, source) in CASES {
        for model in &models {
            for sample in 0..samples {
                let result = credentials.with_provider_key(&model.provider, |key| {
                    translate_text_streaming(
                        TranslateTextRequest {
                            groq_api_key: Credentials::groq_key_for(&model.provider, key),
                            gemini_api_key: key,
                            text: source.to_string(),
                            instruction: INSTRUCTION.to_string(),
                            model: model.full_name.clone(),
                            provider: model.provider.clone(),
                            streaming_enabled: true,
                            use_json_format: false,
                            response_schema: None,
                            search_label: None,
                            ui_language: "en",
                            cancel_token: None,
                            request_timeout: Some(Duration::from_secs(60)),
                            target_language: Some("Vietnamese".to_string()),
                        },
                        |_| {},
                    )
                });
                let (verdict, detail) = match &result {
                    Err(error) => ("ERROR".to_string(), error.to_string()),
                    Ok(reply) if looks_like_english(reply) => {
                        ("ENGLISH".to_string(), reply.chars().take(80).collect())
                    }
                    Ok(reply) if !has_vietnamese_marks(reply) => {
                        ("NOT-VI".to_string(), reply.chars().take(80).collect())
                    }
                    Ok(reply) => {
                        let placeholders = invented_placeholders(reply);
                        if !placeholders.is_empty() {
                            ("TEMPLATE".to_string(), placeholders.join(", "))
                        } else if invented_signoff(reply) {
                            ("SIGNOFF".to_string(), "added a closing".to_string())
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
