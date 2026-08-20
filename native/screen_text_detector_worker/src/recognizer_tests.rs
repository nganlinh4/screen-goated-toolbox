use super::batching::PreparedTextLine;
use super::*;

#[test]
fn ctc_decode_removes_blank_and_repeated_classes() {
    let characters = vec![String::new(), "A".to_string(), "B".to_string()];
    let scores = [0.0, 0.9, 0.1, 0.0, 0.8, 0.2, 0.9, 0.05, 0.05, 0.0, 0.1, 0.9];
    let result = decode(&scores, 4, &characters, false).unwrap();
    assert_eq!(result.text, "AB");
    assert!((result.confidence - 0.9).abs() < 0.001);
}

#[test]
fn compact_ctc_decode_matches_full_candidate_semantics() {
    let characters = vec![String::new(), "A".to_string(), "К".to_string()];
    let scores = [0.9, 0.08, 0.02, 0.7, 0.2, 0.1, 0.8, 0.15, 0.05];
    let indices = [1, 2, 0, 1, 2, 0, 2, 1, 0];
    let result = decode_compact(&scores, &indices, 3, &characters).unwrap();
    assert_eq!(result.text, "AК");
    assert_eq!(result.token_count, 2);
}

#[test]
fn ctc_decode_can_reverse_directional_groups() {
    let characters = vec![
        String::new(),
        "A".to_string(),
        "•".to_string(),
        "B".to_string(),
    ];
    let scores = [
        0.0, 0.9, 0.05, 0.05, 0.0, 0.05, 0.9, 0.05, 0.0, 0.05, 0.05, 0.9,
    ];
    let result = decode(&scores, 3, &characters, true).unwrap();
    assert_eq!(result.text, "B•A");
}

#[test]
fn ctc_decode_retains_close_character_candidates_as_script_evidence() {
    let characters = vec![
        String::new(),
        "K".to_string(),
        "К".to_string(),
        "A".to_string(),
        "А".to_string(),
    ];
    let scores = [0.0, 0.60, 0.39, 0.01, 0.0, 0.0, 0.01, 0.0, 0.58, 0.41];
    let result = decode(&scores, 2, &characters, false).unwrap();
    assert_eq!(result.text, "KA");
    assert!(result.script_evidence.contains(&('К' as u32)));
    assert!(result.script_evidence.contains(&('А' as u32)));
    assert_eq!(result.token_count, 2);
}

#[test]
fn recognition_input_is_stride_aligned_and_bounded() {
    let narrow = prepare(&RgbImage::new(8, 40), PRIMARY_INPUT_WIDTH).unwrap();
    let wide = prepare(&RgbImage::new(4_000, 20), PRIMARY_INPUT_WIDTH).unwrap();
    assert_eq!(narrow.width, MIN_INPUT_WIDTH);
    assert_eq!(wide.width, PRIMARY_INPUT_WIDTH);
}

#[test]
fn extreme_line_is_tiled_without_resizing_any_tile_past_the_cap() {
    let source = RgbImage::from_pixel(1_427, 20, image::Rgb([255, 255, 255]));
    let ranges = recognition_ranges(&source, UNKNOWN_PROBE_INPUT_WIDTH);
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, source.width());
    assert!(ranges.len() > 1);
    assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
    for range in ranges {
        let crop = image::imageops::crop_imm(
            &source,
            range.start,
            0,
            range.end - range.start,
            source.height(),
        )
        .to_image();
        let expected = crop.width() as f64 * f64::from(INPUT_HEIGHT) / f64::from(crop.height());
        assert!(expected <= f64::from(UNKNOWN_PROBE_INPUT_WIDTH));
    }
}

#[test]
fn tiled_results_reassemble_in_source_order_with_weighted_confidence() {
    let plan = RecognitionPlan {
        source_count: 1,
        tiles: vec![
            RecognitionTile {
                source_index: 0,
                image: RgbImage::new(10, 10),
                separated_from_previous: false,
            },
            RecognitionTile {
                source_index: 0,
                image: RgbImage::new(10, 10),
                separated_from_previous: true,
            },
        ],
    };
    let results = plan
        .assemble(
            vec![
                Recognition {
                    text: "first part".to_string(),
                    confidence: 0.9,
                    script_evidence: vec![1],
                    token_count: 9,
                },
                Recognition {
                    text: "second part".to_string(),
                    confidence: 0.6,
                    script_evidence: vec![2],
                    token_count: 10,
                },
            ],
            false,
        )
        .unwrap();
    assert_eq!(results[0].text, "first part second part");
    assert_eq!(results[0].token_count, 19);
    assert!((results[0].confidence - (14.1 / 19.0)).abs() < 0.001);
    assert_eq!(results[0].script_evidence, vec![1, 2]);
}

#[test]
fn batch_tensor_preserves_each_source_plane_and_pads_with_neutral_values() {
    let first = PreparedTextLine {
        width: 32,
        chw: vec![1.0; 32 * INPUT_HEIGHT as usize * 3],
    };
    let second = PreparedTextLine {
        width: 64,
        chw: vec![2.0; 64 * INPUT_HEIGHT as usize * 3],
    };
    let tensor = batch_tensor(&[first, second], &[0, 1], 64).unwrap();
    let plane = 64 * INPUT_HEIGHT as usize;
    assert_eq!(tensor[0], 1.0);
    assert_eq!(tensor[31], 1.0);
    assert_eq!(tensor[32], 0.0);
    assert_eq!(tensor[plane * 3], 2.0);
}
