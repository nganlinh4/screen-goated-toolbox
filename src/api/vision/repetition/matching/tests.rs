use super::PreviousMatches;

fn oracle(chars: &[char], start: usize) -> usize {
    (1..=start.min(chars.len() - start))
        .filter(|&length| {
            chars[..start]
                .windows(length)
                .any(|w| w == &chars[start..start + length])
        })
        .max()
        .unwrap_or(0)
}

#[test]
fn cached_matches_equal_exhaustive_nonoverlapping_search() {
    for length in 0..=7 {
        for mut code in 0..3usize.pow(length) {
            let chars: Vec<char> = (0..length)
                .map(|_| {
                    let ch = ['a', 'b', '🌱'][code % 3];
                    code /= 3;
                    ch
                })
                .collect();
            let matches = PreviousMatches::new(&chars);
            let expected: Vec<_> = (0..chars.len())
                .map(|start| oracle(&chars, start))
                .collect();
            for start in 0..chars.len() {
                assert_eq!(matches.span(start), expected[start], "{chars:?} at {start}");
                for minimum in 1..=3 {
                    assert_eq!(
                        matches.has_anchor(start, minimum),
                        expected[start..].iter().any(|&n| n >= minimum)
                    );
                    for end in start..=chars.len() {
                        let mut covered = 0;
                        let mut index = start;
                        while index < end {
                            let span = oracle(&chars, index);
                            if span >= minimum {
                                covered += span.min(end - index);
                                index += span;
                            } else {
                                index += 1;
                            }
                        }
                        let coverage = if end == start {
                            0.0
                        } else {
                            covered as f32 / (end - start) as f32
                        };
                        assert_eq!(matches.coverage(start, end, minimum), coverage);
                    }
                }
            }
        }
    }
}

#[test]
fn shared_long_replies_keep_all_legitimate_text() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/preset-system/vision-repetition-work.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let text = case["text"]
            .as_str()
            .unwrap()
            .repeat(case["repeat"].as_u64().unwrap() as usize);
        let mut guard = crate::api::vision::repetition::RepetitionGuard::default();
        for ch in text.chars() {
            guard.observe(&ch.to_string());
        }
        assert_eq!(guard.finish(text.clone()), text);
    }
}
