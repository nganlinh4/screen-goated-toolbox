use std::collections::HashSet;

pub(super) const DEFAULT_VIDEO_QUALITY: &str = "1080p";
const SMALLER_VIDEO_QUALITY: &str = "720p";
pub(super) const COMPLETED_QUALITY_PREFIX: &str = "SGT_VIDEO_QUALITY ";
pub(super) const COMPLETED_QUALITY_TEMPLATE: &str =
    "after_move:SGT_VIDEO_QUALITY %(.{width,height})j";

pub(super) fn choices(remembered: Option<&str>, formats: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    [DEFAULT_VIDEO_QUALITY, SMALLER_VIDEO_QUALITY]
        .into_iter()
        .chain(remembered)
        .chain(formats.iter().map(String::as_str))
        .filter(|&quality| seen.insert(quality))
        .map(str::to_owned)
        .collect()
}

pub(super) fn reported_quality(record: &str) -> Option<u32> {
    let report: serde_json::Value = serde_json::from_str(record).ok()?;
    let dimension = |key: &str| {
        u32::try_from(report[key].as_u64()?)
            .ok()
            .filter(|&value| value > 0)
    };
    match (dimension("width"), dimension("height")) {
        (Some(width), Some(height)) => Some(width.min(height)),
        _ => None,
    }
}

pub(super) fn common_quality(qualities: &HashSet<Option<u32>>) -> Option<u32> {
    (qualities.len() == 1)
        .then(|| qualities.iter().next().copied().flatten())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_and_completed_qualities_match_shared_cases() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/video-downloader/quality-preferences.json"
        )))
        .unwrap();
        assert_eq!(fixture["freshPreference"], DEFAULT_VIDEO_QUALITY);
        assert_eq!(
            choices(None, &[]),
            [DEFAULT_VIDEO_QUALITY, SMALLER_VIDEO_QUALITY]
        );
        assert_eq!(
            choices(Some("1440p"), &["720p".into(), "480p".into()]),
            ["1080p", "720p", "1440p", "480p"]
        );
        for case in fixture["completed"].as_array().unwrap() {
            let qualities = case["reports"]
                .as_array()
                .unwrap()
                .iter()
                .map(|report| reported_quality(&report.to_string()))
                .collect();
            assert_eq!(
                common_quality(&qualities).map(u64::from),
                case["resolution"].as_u64()
            );
        }
    }
}
