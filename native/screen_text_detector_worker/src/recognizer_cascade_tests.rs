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
    assert!(model.matches_capture(&results));
    assert!(model.matches_capture(&primary_sets(vec![ambiguous])));
    assert!(model.matches_capture(&primary_sets(vec![Recognition {
        text: "号合号合".to_string(),
        confidence: 0.99,
        script_evidence: Vec::new(),
        token_count: 4,
    }])));
}

#[test]
fn specialist_work_is_limited_to_text_line_geometry() {
    assert!(is_text_line_candidate(&RgbImage::new(120, 30)));
    assert!(!is_text_line_candidate(&RgbImage::new(30, 30)));
}

#[test]
fn unknown_script_probe_requires_capture_level_evidence() {
    let weak = primary_sets(vec![
        Recognition {
            text: "V".to_string(),
            confidence: 0.57,
            script_evidence: Vec::new(),
            token_count: 1,
        };
        5
    ]);
    assert!(unknown_probe_needed(
        &[true, true, true, false, false],
        &weak
    ));
    assert!(!unknown_probe_needed(
        &[true, true, false, false, false],
        &weak
    ));
    let mut dense = vec![false; 100];
    dense[3] = true;
    dense[71] = true;
    let dense_results = primary_sets(vec![
        Recognition {
            text: "Settings".to_string(),
            confidence: 0.99,
            script_evidence: Vec::new(),
            token_count: 8,
        };
        100
    ]);
    assert!(!unknown_probe_needed(&dense, &dense_results));
    let readable_vertical = primary_sets(vec![
        Recognition {
            text: "百科".to_string(),
            confidence: 0.95,
            script_evidence: Vec::new(),
            token_count: 2,
        };
        5
    ]);
    assert!(unknown_probe_needed(
        &[true, true, true, false, false],
        &readable_vertical
    ));
}
