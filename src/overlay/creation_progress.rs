use serde_json::Value;

const MAX_TIMING_SAMPLE_COUNT: u64 = 100_000;

pub(crate) fn elapsed_ms(value: &Value) -> Option<u64> {
    bounded_u64(
        value,
        "elapsedMs",
        crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS,
    )
}

pub(crate) fn estimated_total_ms(value: &Value) -> Option<u64> {
    bounded_u64(
        value,
        "estimatedTotalMs",
        crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS,
    )
}

pub(crate) fn ratio(value: &Value) -> Option<f64> {
    value
        .get("progressRatio")
        .and_then(Value::as_f64)
        .filter(|ratio| ratio.is_finite())
        .map(|ratio| ratio.clamp(0.0, 1.0))
}

pub(crate) fn timing_sample_count(value: &Value) -> Option<u64> {
    bounded_u64(value, "timingSampleCount", MAX_TIMING_SAMPLE_COUNT)
}

fn bounded_u64(value: &Value, field: &str, maximum: u64) -> Option<u64> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|number| number.min(maximum))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{elapsed_ms, estimated_total_ms, ratio, timing_sample_count};

    #[test]
    fn progress_numbers_are_bounded_and_invalid_values_are_ignored() {
        let huge = json!({
            "elapsedMs": u64::MAX,
            "estimatedTotalMs": u64::MAX,
            "progressRatio": 50.0,
            "timingSampleCount": u64::MAX,
        });
        assert_eq!(elapsed_ms(&huge), Some(7_200_000));
        assert_eq!(estimated_total_ms(&huge), Some(7_200_000));
        assert_eq!(ratio(&huge), Some(1.0));
        assert_eq!(timing_sample_count(&huge), Some(100_000));

        let invalid = json!({
            "elapsedMs": -1,
            "estimatedTotalMs": "unknown",
            "progressRatio": -4.0,
            "timingSampleCount": null,
        });
        assert_eq!(elapsed_ms(&invalid), None);
        assert_eq!(estimated_total_ms(&invalid), None);
        assert_eq!(ratio(&invalid), Some(0.0));
        assert_eq!(timing_sample_count(&invalid), None);
    }
}
