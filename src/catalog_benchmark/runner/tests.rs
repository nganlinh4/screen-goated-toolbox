use super::{TimingMetrics, load_ocr_image, reasoning_policy_label};
use crate::catalog_benchmark::manifest::{Manifest, OcrInputMode};
use crate::model_config::get_model_by_id;

#[test]
fn ocr_runtime_inputs_apply_manifest_crops() {
    let manifest = Manifest::load().unwrap();
    for case in &manifest.ocr_cases {
        let (image, encoded) = load_ocr_image(
            &manifest.image_path(&case.image),
            case.crop_px,
            case.input_mode,
        )
        .unwrap();
        if let Some([_, _, width, height]) = case.crop_px {
            assert_eq!((image.width(), image.height()), (width, height));
            let decoded = image::load_from_memory(&encoded).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (width, height));
        }
        match case.input_mode {
            OcrInputMode::ScreenCropPng => {
                assert!(encoded.starts_with(&[0x89, b'P', b'N', b'G']));
            }
            OcrInputMode::OriginalFile => {
                assert_eq!(
                    encoded,
                    std::fs::read(manifest.image_path(&case.image)).unwrap()
                );
            }
        }
    }
}

#[test]
fn canonical_ocr_cases_match_the_built_in_rust_preset() {
    let presets = crate::config::preset::defaults::create_image_presets();
    let preset = presets
        .iter()
        .find(|preset| preset.id == "preset_ocr")
        .expect("built-in Extract text preset");
    let block = preset.blocks.first().expect("OCR image block");
    assert_eq!(
        block.prompt,
        crate::config::preset::defaults::OCR_EXTRACTION_PROMPT
    );
    assert_eq!(block.render_mode, "markdown");
    assert!(!block.streaming_enabled);

    let manifest = Manifest::load().unwrap();
    let canonical = manifest
        .ocr_cases
        .iter()
        .filter(|case| case.instruction == block.prompt)
        .collect::<Vec<_>>();
    assert_eq!(canonical.len(), 3);
    assert_eq!(
        canonical
            .iter()
            .filter(|case| case.crop_px.is_some())
            .count(),
        2
    );
}

#[test]
fn visual_review_uses_the_manifest_boxes_crops_and_input_modes() {
    let manifest = Manifest::load().unwrap();
    let review = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/catalog-benchmark/review.html"),
    )
    .unwrap();

    for case in &manifest.coordinate_cases {
        let box_px = case
            .box_px
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let marker = format!("data-case-id=\"{}\" data-box-px=\"{box_px}\"", case.id);
        assert!(
            review.contains(&marker),
            "review box drifted for {}",
            case.id
        );
    }

    for case in &manifest.ocr_cases {
        let marker = format!(
            "data-case-id=\"{}\" data-input-mode=\"{}\"",
            case.id,
            case.input_mode.as_str()
        );
        assert!(
            review.contains(&marker),
            "review input mode drifted for {}",
            case.id
        );
        if let Some(crop_px) = case.crop_px {
            let crop = crop_px
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            assert!(
                review.contains(&format!("data-crop-px=\"{crop}\"")),
                "review crop marker drifted for {}",
                case.id
            );
            assert!(
                review.contains(&format!("viewBox=\"{}\"", crop.replace(',', " "))),
                "review crop viewport drifted for {}",
                case.id
            );
        }
    }
}

#[test]
fn response_timing_ignores_thinking_and_accepts_the_wipe_transition() {
    let events = vec![
        (9, "Thinking…".to_string()),
        (30, format!("{}Hello", crate::api::WIPE_SIGNAL)),
        (55, " world".to_string()),
    ];
    let timing = TimingMetrics::for_response(100, &events, "Hello world");

    assert_eq!(timing.time_to_first_output_ms, Some(30));
    assert_eq!(timing.generation_duration_ms, Some(70));
    assert_eq!(timing.output_chars, Some(11));
    assert_eq!(timing.end_to_end_chars_per_second, Some(110.0));
    assert!((timing.generation_chars_per_second.unwrap() - (6.0 / 0.07)).abs() < f64::EPSILON);
}

#[test]
fn non_streaming_response_reports_completion_as_first_output() {
    let timing = TimingMetrics::for_response(750, &[], "done");

    assert_eq!(timing.time_to_first_output_ms, Some(750));
    assert_eq!(timing.generation_duration_ms, Some(0));
    assert_eq!(timing.generation_chars_per_second, None);
}

#[test]
fn attempts_fingerprint_the_current_production_reasoning_policy() {
    let cases = [
        ("google-gemini-3-5-flash-lite-text", "gemini-level:minimal"),
        ("google-gemini-2-5-flash-lite-text", "gemini-budget:0"),
        ("groq-qwen-3-6-27b-vision", "openai-effort:none"),
        ("cerebras-gpt-oss-120b-text", "openai-effort:low"),
    ];
    for (id, expected) in cases {
        let model = get_model_by_id(id).unwrap();
        assert_eq!(reasoning_policy_label(&model), expected, "{id}");
    }
}
