use super::*;
use crate::recognizer::Recognition;
use crate::recognizer_cascade::RecognitionSet;

#[test]
fn inference_dimensions_are_stride_aligned_and_bounded() {
    assert_eq!(inference_size(651, 398), (640, 384));
    let tall = inference_size(1_080, 1_920);
    assert_eq!(tall.0 % 32, 0);
    assert_eq!(tall.1, INFERENCE_LONG_SIDE);
    let four_k = inference_size(3_840, 2_160);
    assert_eq!(four_k.0 % 32, 0);
    assert_eq!(four_k.1 % 32, 0);
    assert_eq!(four_k.0.max(four_k.1), INFERENCE_LONG_SIDE);
}

#[test]
fn orientation_candidates_follow_crop_geometry_not_language() {
    assert!(needs_orientation_candidates(40, 100));
    assert!(!needs_orientation_candidates(100, 40));
    assert!(!needs_orientation_candidates(100, 150));
}

#[test]
fn recognition_selection_balances_sequence_evidence_and_confidence() {
    let fragment = RecognitionSet {
        primary: Recognition {
            text: "ab".to_string(),
            confidence: 0.96,
            script_evidence: Vec::new(),
            token_count: 2,
        },
        alternatives: Vec::new(),
    };
    let complete = RecognitionSet {
        primary: Recognition {
            text: "complete text".to_string(),
            confidence: 0.82,
            script_evidence: Vec::new(),
            token_count: 13,
        },
        alternatives: Vec::new(),
    };
    let selected = merge_recognition_sets(fragment, complete);
    assert_eq!(selected.primary.text, "complete text");
    assert_eq!(selected.alternatives.len(), 2);
}

#[test]
fn protocol_output_is_bounded_deduplicated_and_utf8_safe() {
    let mut region = DetectedRegion {
        left: 0,
        top: 0,
        right: 10,
        bottom: 10,
        confidence: 0.9,
        text: String::new(),
        text_confidence: 0.0,
        alternatives: Vec::new(),
    };
    let primary = Recognition {
        text: "한".repeat(MAX_REGION_TEXT_BYTES),
        confidence: f32::NAN,
        script_evidence: Vec::new(),
        token_count: MAX_REGION_TEXT_BYTES,
    };
    let alternatives = std::iter::once(primary.clone())
        .chain(
            (0..MAX_RECOGNITION_ALTERNATIVES + 4).map(|index| Recognition {
                text: format!("candidate-{index}"),
                confidence: 1.5,
                script_evidence: Vec::new(),
                token_count: 11,
            }),
        )
        .collect();

    apply_recognition(
        &mut region,
        RecognitionSet {
            primary,
            alternatives,
        },
    );

    assert!(region.text.len() <= MAX_REGION_TEXT_BYTES);
    assert!(region.text.is_char_boundary(region.text.len()));
    assert_eq!(region.text_confidence, 0.0);
    assert_eq!(region.alternatives.len(), MAX_RECOGNITION_ALTERNATIVES);
    assert!(
        region
            .alternatives
            .iter()
            .all(|candidate| candidate.confidence == 1.0 && candidate.text != region.text)
    );
    sgt_screen_text_detector_protocol::write_server(
        &mut Vec::new(),
        1,
        &sgt_screen_text_detector_protocol::ServerMessage::Regions {
            image_width: 10,
            image_height: 10,
            timings: DetectionTimings::default(),
            regions: vec![region],
        },
    )
    .expect("normalized recognition output must satisfy the wire contract");
}
