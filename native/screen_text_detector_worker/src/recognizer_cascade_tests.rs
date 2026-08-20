use super::*;

#[test]
fn fast_path_requires_confident_text() {
    assert!(!needs_alternatives(&Recognition {
        text: "ordinary text".to_string(),
        confidence: FAST_PATH_CONFIDENCE,
        script_evidence: Vec::new(),
        token_count: 13,
    }));
    assert!(needs_alternatives(&Recognition {
        text: String::new(),
        confidence: 1.0,
        script_evidence: Vec::new(),
        token_count: 0,
    }));
    assert!(needs_alternatives(&Recognition {
        text: "«»".to_string(),
        confidence: 0.99,
        script_evidence: Vec::new(),
        token_count: 2,
    }));
}

#[test]
fn specialist_coverage_routes_only_matching_unicode() {
    let coverage = [[0x0400, 0x052f]];
    assert!(coverage_matches(&coverage, "Текст"));
    assert!(!coverage_matches(&coverage, "Text"));
    assert!(!coverage_matches(&[], "Текст"));
}

#[test]
fn repeated_decoder_evidence_routes_a_confident_wrong_script() {
    let coverage = [[0x0400, 0x052f]];
    let result = Recognition {
        text: "KAMEHE".to_string(),
        confidence: 0.94,
        script_evidence: vec!['К' as u32, 'А' as u32, 'М' as u32],
        token_count: 6,
    };
    assert!(evidence_matches(&coverage, &result));

    let stray = Recognition {
        script_evidence: vec!['К' as u32],
        ..result
    };
    assert!(!evidence_matches(&coverage, &stray));
}

#[test]
fn specialist_output_requires_confidence_and_its_own_script() {
    let coverage = [[0x0400, 0x052f]];
    let usable = Recognition {
        text: "Текст".to_string(),
        confidence: SPECIALIST_ALTERNATIVE_CONFIDENCE,
        script_evidence: Vec::new(),
        token_count: 5,
    };
    assert!(specialist_alternative_is_usable(&coverage, &usable));
    assert!(!specialist_alternative_is_usable(
        &coverage,
        &Recognition {
            text: "Tekst".to_string(),
            ..usable.clone()
        }
    ));
    assert!(!specialist_alternative_is_usable(
        &coverage,
        &Recognition {
            confidence: SPECIALIST_ALTERNATIVE_CONFIDENCE - 0.01,
            ..usable
        }
    ));
}

#[test]
fn known_capture_script_loads_its_matching_specialist_even_when_primary_is_confident() {
    let coverage = [[0x4e00, 0x9fff]];
    let model = ResolvedModel {
        model: PathBuf::new(),
        cpu_model: None,
        config: PathBuf::new(),
        reverse_output: false,
        coverage: coverage.to_vec(),
        routing: coverage.to_vec(),
    };
    let ambiguous = Recognition {
        text: "号合号合".to_string(),
        confidence: 0.67,
        script_evidence: Vec::new(),
        token_count: 4,
    };
    let weak = Recognition {
        text: String::new(),
        confidence: 0.0,
        script_evidence: Vec::new(),
        token_count: 0,
    };
    let results = primary_sets(vec![ambiguous.clone(), weak]);
    assert!(model.matches_capture(&results, &[0, 1]));
    assert!(model.matches_capture(&primary_sets(vec![ambiguous]), &[0]));
    assert!(model.matches_capture(
        &primary_sets(vec![Recognition {
            text: "号合号合".to_string(),
            confidence: 0.99,
            script_evidence: Vec::new(),
            token_count: 4,
        }]),
        &[0]
    ));
}

#[test]
fn capture_routing_rejects_one_weak_script_hallucination() {
    let routing = [[0x0400, 0x052f]];
    let weak = primary_sets(vec![Recognition {
        text: "Ю".to_string(),
        confidence: 0.55,
        script_evidence: Vec::new(),
        token_count: 1,
    }]);
    assert!(!capture_routing_matches(&routing, &weak));

    let strong = primary_sets(vec![Recognition {
        text: "Юрий".to_string(),
        confidence: 0.91,
        script_evidence: Vec::new(),
        token_count: 4,
    }]);
    assert!(capture_routing_matches(&routing, &strong));
}

#[test]
fn decoder_evidence_cannot_override_an_explicit_other_non_ascii_script() {
    let routing = [[0xac00, 0xd7af]];
    let result = primary_sets(vec![Recognition {
        text: "漢字".to_string(),
        confidence: 0.98,
        script_evidence: vec!['한' as u32, '글' as u32],
        token_count: 2,
    }]);
    assert!(!capture_routing_matches(&routing, &result));
}

#[test]
fn specialist_work_is_limited_to_text_line_geometry() {
    assert!(is_text_line_candidate(&RgbImage::new(120, 30)));
    assert!(!is_text_line_candidate(&RgbImage::new(30, 30)));
}

#[test]
fn unknown_script_probe_requires_capture_level_evidence() {
    let sources = (0..5).map(|_| RgbImage::new(120, 30)).collect::<Vec<_>>();
    assert!(unknown_probe::needed(
        &[true, true, true, false, false],
        &sources,
        &[0, 1, 2, 3, 4]
    ));
    assert!(!unknown_probe::needed(
        &[true, true, false, false, false],
        &sources,
        &[0, 1, 2, 3, 4]
    ));
    let mut dense = vec![false; 100];
    dense[3] = true;
    dense[71] = true;
    let dense_sources = (0..100).map(|_| RgbImage::new(120, 30)).collect::<Vec<_>>();
    assert!(!unknown_probe::needed(
        &dense,
        &dense_sources,
        &(0..100).collect::<Vec<_>>()
    ));
    assert!(unknown_probe::needed(
        &[true, true, true, false, false],
        &sources,
        &[0, 1, 2, 3, 4]
    ));
}

#[test]
fn one_or_two_unresolved_lines_probe_only_when_visually_dominant() {
    assert!(unknown_probe::needed(
        &[false, true, true, false],
        &[
            RgbImage::new(180, 24),
            RgbImage::new(1_400, 24),
            RgbImage::new(700, 24),
            RgbImage::new(80, 24),
        ],
        &[0, 1, 2, 3]
    ));
    assert!(!unknown_probe::needed(
        &[false, true, false, false],
        &[
            RgbImage::new(1_400, 24),
            RgbImage::new(80, 24),
            RgbImage::new(900, 24),
            RgbImage::new(700, 24),
        ],
        &[0, 1, 2, 3]
    ));
}

#[test]
fn routing_uses_only_the_best_orientation_per_region() {
    let results = primary_sets(vec![
        Recognition {
            text: "readable text".to_string(),
            confidence: 0.95,
            script_evidence: Vec::new(),
            token_count: 12,
        },
        Recognition {
            text: String::new(),
            confidence: 0.0,
            script_evidence: Vec::new(),
            token_count: 0,
        },
    ]);
    assert_eq!(representative_indices(&results, &[7, 7]), [0]);
}
