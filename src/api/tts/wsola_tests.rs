use super::WsolaStretcher;

#[test]
fn shared_stream_finalization_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/wsola-finalization.json"
    )))
    .unwrap();
    let samples = |name: &str| -> Vec<i16> {
        fixture[name]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sample| sample.as_i64().unwrap() as i16)
            .collect()
    };
    let first = samples("shortStream");
    let second = samples("replacementStream");
    let speed = fixture["shortStream"]["speed"].as_f64().unwrap();
    let mut stretcher = WsolaStretcher::new(24_000);
    assert!(stretcher.stretch(&first, speed).is_empty());
    assert_eq!(stretcher.finish(), first);
    assert!(stretcher.finish().is_empty());
    assert!(stretcher.stretch(&first, speed).is_empty());
    stretcher.reset();
    assert!(stretcher.stretch(&second, speed).is_empty());
    assert_eq!(stretcher.finish(), second);
    assert!(stretcher.stretch(&first, speed).is_empty());
    let combined = [first, second.clone()].concat();
    assert_eq!(
        stretcher.stretch(&second, fixture["normalSpeed"].as_f64().unwrap()),
        combined
    );
}
