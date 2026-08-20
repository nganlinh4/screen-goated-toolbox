//! Persistent host client for detector-owned Screen Translate geometry.

mod client;
mod process;

use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
use sgt_screen_text_detector_protocol::DetectedRegion;

use super::contract::{
    DetectedTextRegion, MAX_CANDIDATES, MAX_SOURCE_CANDIDATES, NormalizedBounds,
    RecognitionEvidence,
};

static CLIENT: LazyLock<Mutex<Option<client::DetectorClient>>> = LazyLock::new(|| Mutex::new(None));
const MIN_TEXT_CONFIDENCE: f32 = 0.5;

pub(super) struct DetectionBatch {
    pub accepted: Vec<DetectedTextRegion>,
    pub raw: Vec<DetectedRegion>,
}

pub(super) fn prepare(cancelled: std::sync::Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("sgt-screen-text-detector-prepare".to_string())
        .spawn(move || {
            let mut client = CLIENT.lock().unwrap_or_else(|value| value.into_inner());
            if client.is_none() {
                match client::DetectorClient::start(&cancelled) {
                    Ok(started) => *client = Some(started),
                    Err(error) if !cancelled.load(std::sync::atomic::Ordering::SeqCst) => {
                        crate::log_info!(
                            "[Screen Translate] detector preparation failed: {error:#}"
                        );
                    }
                    Err(_) => {}
                }
            }
        });
}

pub(super) fn detect(
    jpeg: &[u8],
    expected_width: u32,
    expected_height: u32,
    cancelled: &AtomicBool,
) -> Result<DetectionBatch> {
    let mut client = CLIENT.lock().unwrap_or_else(|value| value.into_inner());
    if client.is_none() {
        *client = Some(client::DetectorClient::start(cancelled)?);
    }
    let result = client
        .as_mut()
        .expect("detector client initialized")
        .detect(jpeg, cancelled);
    let (width, height, mut regions) = match result {
        Ok(response) => response,
        Err(error) => {
            client.take();
            return Err(error);
        }
    };
    if width != expected_width || height != expected_height {
        client.take();
        bail!(
            "text detector image dimensions changed: expected {expected_width}x{expected_height}, got {width}x{height}"
        );
    }

    let raw = regions.clone();
    regions.retain(|region| {
        region.right - region.left >= super::geometry::MIN_READABLE_WIDTH
            && region.bottom - region.top >= super::geometry::MIN_READABLE_HEIGHT
            && has_readable_recognition(region)
    });
    remove_adjacent_icon_recognitions(&mut regions);
    if regions.len() > MAX_CANDIDATES {
        regions.sort_by(|left, right| {
            right
                .confidence
                .total_cmp(&left.confidence)
                .then_with(|| area(right).cmp(&area(left)))
        });
        regions.truncate(MAX_CANDIDATES);
    }
    regions.sort_by_key(|region| (region.top, region.left, region.bottom, region.right));
    let accepted = regions
        .into_iter()
        .enumerate()
        .map(|(index, region)| {
            let ranked = recognition_candidates(&region);
            let source_alternatives = ranked.iter().map(|item| item.0.clone()).collect::<Vec<_>>();
            DetectedTextRegion {
                id: u16::try_from(index + 1).expect("candidate cap fits u16"),
                bounds: normalized_bounds(&region, width, height),
                source_text: source_alternatives[0].clone(),
                source_alternatives,
                recognition: RecognitionEvidence {
                    locator_confidence: region.confidence,
                    selected_confidence: ranked[0].1,
                    competing_confidence: ranked.get(1).map_or(0.0, |item| item.1),
                },
                appearance: None,
            }
        })
        .collect();
    Ok(DetectionBatch { accepted, raw })
}

fn has_readable_recognition(region: &DetectedRegion) -> bool {
    std::iter::once((region.text.as_str(), region.text_confidence))
        .chain(
            region
                .alternatives
                .iter()
                .map(|candidate| (candidate.text.as_str(), candidate.confidence)),
        )
        .any(|(text, confidence)| {
            confidence >= MIN_TEXT_CONFIDENCE && text.chars().any(char::is_alphabetic)
        })
}

fn remove_adjacent_icon_recognitions(regions: &mut Vec<DetectedRegion>) {
    let snapshot = regions.clone();
    regions.retain(|region| {
        let width = region.right.saturating_sub(region.left);
        let height = region.bottom.saturating_sub(region.top).max(1);
        let compact =
            width <= height.saturating_mul(3) / 2 && height <= width.max(1).saturating_mul(3) / 2;
        let is_leading_icon = compact
            && snapshot.iter().any(|neighbor| {
                let neighbor_width = neighbor.right.saturating_sub(neighbor.left);
                let horizontal_gap = neighbor.left.saturating_sub(region.right);
                let vertical_overlap = region
                    .bottom
                    .min(neighbor.bottom)
                    .saturating_sub(region.top.max(neighbor.top));
                neighbor.left >= region.right
                    && horizontal_gap <= height
                    && neighbor_width >= height.saturating_mul(2)
                    && vertical_overlap.saturating_mul(2)
                        >= height.min(neighbor.bottom.saturating_sub(neighbor.top).max(1))
            });
        !is_leading_icon
    });
}

fn recognition_candidates(region: &DetectedRegion) -> Vec<(String, f32)> {
    let mut ranked = std::iter::once((region.text.as_str(), region.text_confidence))
        .chain(
            region
                .alternatives
                .iter()
                .map(|candidate| (candidate.text.as_str(), candidate.confidence)),
        )
        .filter_map(|(text, confidence)| {
            let text = text.trim();
            (!text.is_empty() && confidence >= MIN_TEXT_CONFIDENCE).then_some((text, confidence))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        recognition_quality(right.0, right.1)
            .total_cmp(&recognition_quality(left.0, left.1))
            .then_with(|| right.1.total_cmp(&left.1))
    });
    let mut candidates: Vec<(String, f32)> = Vec::with_capacity(MAX_SOURCE_CANDIDATES);
    let Some((primary_text, primary_confidence)) = ranked.first().copied() else {
        return candidates;
    };
    candidates.push((primary_text.to_string(), primary_confidence));
    let alternative = ranked
        .into_iter()
        .skip(1)
        .filter(|(text, _)| *text != primary_text)
        .max_by(|left, right| {
            alternative_quality(primary_text, left.0, left.1).total_cmp(&alternative_quality(
                primary_text,
                right.0,
                right.1,
            ))
        });
    if let Some((text, confidence)) = alternative {
        candidates.push((text.to_string(), confidence));
    }
    candidates
}

fn recognition_quality(text: &str, confidence: f32) -> f32 {
    let useful_characters = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count()
        .clamp(1, 2_000) as f32;
    useful_characters + confidence.clamp(0.0, 1.0)
}

fn alternative_quality(primary: &str, alternative: &str, confidence: f32) -> f32 {
    let primary_buckets = alphabetic_buckets(primary);
    let complementary_buckets = alphabetic_buckets(alternative)
        .difference(&primary_buckets)
        .count() as f32;
    complementary_buckets * 4_000.0 + recognition_quality(alternative, confidence)
}

fn alphabetic_buckets(text: &str) -> std::collections::HashSet<u32> {
    text.chars()
        .filter(|character| character.is_alphabetic())
        .map(|character| u32::from(character) >> 8)
        .collect()
}

pub(super) fn stop() {
    CLIENT
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .take();
}

fn area(region: &DetectedRegion) -> u64 {
    u64::from(region.right - region.left) * u64::from(region.bottom - region.top)
}

fn normalized_bounds(region: &DetectedRegion, width: u32, height: u32) -> NormalizedBounds {
    fn coordinate(value: u32, extent: u32) -> u16 {
        let scaled = (u64::from(value) * 1000 + u64::from(extent) / 2) / u64::from(extent);
        scaled.min(1000) as u16
    }
    fn axis(start: u16, end: u16) -> (u16, u16) {
        if end > start {
            return (start, end);
        }
        if start < 1000 {
            (start, start + 1)
        } else {
            (999, 1000)
        }
    }
    let (left, right) = axis(
        coordinate(region.left, width),
        coordinate(region.right, width),
    );
    let (top, bottom) = axis(
        coordinate(region.top, height),
        coordinate(region.bottom, height),
    );
    NormalizedBounds {
        left,
        top,
        right,
        bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_boxes_map_both_width_and_height_to_the_normalized_grid() {
        let bounds = normalized_bounds(
            &DetectedRegion {
                left: 100,
                top: 50,
                right: 500,
                bottom: 250,
                confidence: 0.9,
                text: "Settings".to_string(),
                text_confidence: 0.95,
                alternatives: Vec::new(),
            },
            1_000,
            500,
        );
        assert_eq!(bounds.left, 100);
        assert_eq!(bounds.right, 500);
        assert_eq!(bounds.top, 100);
        assert_eq!(bounds.bottom, 500);
    }

    #[test]
    fn recognition_candidates_are_complete_deduplicated_and_bounded() {
        let region = DetectedRegion {
            left: 0,
            top: 0,
            right: 100,
            bottom: 20,
            confidence: 0.9,
            text: "weak".to_string(),
            text_confidence: 0.6,
            alternatives: vec![
                sgt_screen_text_detector_protocol::RecognitionAlternative {
                    text: "correct".to_string(),
                    confidence: 0.98,
                },
                sgt_screen_text_detector_protocol::RecognitionAlternative {
                    text: "correct".to_string(),
                    confidence: 0.9,
                },
                sgt_screen_text_detector_protocol::RecognitionAlternative {
                    text: "second".to_string(),
                    confidence: 0.8,
                },
                sgt_screen_text_detector_protocol::RecognitionAlternative {
                    text: "fourth".to_string(),
                    confidence: 0.7,
                },
            ],
        };
        assert_eq!(
            recognition_candidates(&region)
                .into_iter()
                .map(|item| item.0)
                .collect::<Vec<_>>(),
            vec!["correct", "second"]
        );
    }

    #[test]
    fn recognition_ranking_does_not_prefer_a_confident_fragment_over_a_complete_sequence() {
        let region = DetectedRegion {
            left: 0,
            top: 0,
            right: 20,
            bottom: 100,
            confidence: 0.9,
            text: "ab".to_string(),
            text_confidence: 0.96,
            alternatives: vec![sgt_screen_text_detector_protocol::RecognitionAlternative {
                text: "complete text".to_string(),
                confidence: 0.82,
            }],
        };
        assert_eq!(recognition_candidates(&region)[0].0, "complete text");
    }

    #[test]
    fn one_complementary_reading_wins_the_bounded_alternative_slot() {
        let region = DetectedRegion {
            left: 0,
            top: 0,
            right: 100,
            bottom: 20,
            confidence: 0.9,
            text: "alpha beta".to_string(),
            text_confidence: 0.9,
            alternatives: vec![
                sgt_screen_text_detector_protocol::RecognitionAlternative {
                    text: "alpha beto".to_string(),
                    confidence: 0.95,
                },
                sgt_screen_text_detector_protocol::RecognitionAlternative {
                    text: "alpha βeta".to_string(),
                    confidence: 0.8,
                },
            ],
        };
        assert_eq!(recognition_candidates(&region)[1].0, "alpha βeta");
    }

    #[test]
    fn compact_leading_icon_recognition_does_not_become_translation_text() {
        let region = |left, top, right, bottom, text: &str| DetectedRegion {
            left,
            top,
            right,
            bottom,
            confidence: 0.9,
            text: text.to_string(),
            text_confidence: 0.9,
            alternatives: Vec::new(),
        };
        let mut regions = vec![
            region(10, 10, 30, 30, "noise"),
            region(36, 10, 116, 30, "Readable label"),
            region(10, 60, 30, 80, "Real"),
        ];
        remove_adjacent_icon_recognitions(&mut regions);
        assert_eq!(
            regions
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
            ["Readable label", "Real"]
        );
    }
}
