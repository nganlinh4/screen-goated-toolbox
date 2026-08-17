use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result, bail};
use image::imageops::FilterType;
use image::{ImageReader, Limits, RgbImage};
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use sgt_screen_text_detector_protocol::{
    DetectedRegion, MAX_RECOGNITION_ALTERNATIVES, MAX_REGION_TEXT_BYTES, RecognitionAlternative,
};

use crate::postprocess;
use crate::recognizer::Acceleration;
use crate::recognizer_cascade::RecognizerCascade;

const MAX_IMAGE_SIDE: u32 = 8_192;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const INFERENCE_LONG_SIDE: u32 = 1_600;
const WARMUP_SIDE: usize = 320;

pub(crate) struct DetectionResult {
    pub(crate) image_width: u32,
    pub(crate) image_height: u32,
    pub(crate) regions: Vec<DetectedRegion>,
}

pub(crate) struct TextDetector {
    cpu: DetectorBackend,
    direct_ml_recognizer: Option<RecognizerCascade>,
    direct_ml_locator: Option<TextLocator>,
}

struct DetectorBackend {
    locator: TextLocator,
    recognizer: RecognizerCascade,
}

struct TextLocator {
    session: Session,
}

struct LocatedImage {
    image_width: u32,
    image_height: u32,
    regions: Vec<DetectedRegion>,
    crops: Vec<RgbImage>,
    crop_region_indices: Vec<usize>,
}

impl TextDetector {
    pub(crate) fn load(
        cpu_detector: &Path,
        direct_ml_detector: &Path,
        model_root: &Path,
        recognizer_catalog: &Path,
    ) -> Result<Self> {
        let cpu = DetectorBackend::load(cpu_detector, model_root, recognizer_catalog)?;
        let direct_ml_locator = optional_acceleration(
            TextLocator::load(direct_ml_detector, Acceleration::DirectMl).and_then(
                |mut locator| {
                    locator.warm_up()?;
                    Ok(locator)
                },
            ),
            "locator",
        );
        let direct_ml_recognizer = direct_ml_locator.as_ref().and_then(|_| {
            optional_acceleration(
                RecognizerCascade::load(
                    model_root,
                    recognizer_catalog,
                    Acceleration::DirectMl,
                    false,
                )
                .and_then(|mut recognizer| {
                    recognizer.warm_all()?;
                    Ok(recognizer)
                }),
                "recognizer",
            )
        });
        Ok(Self {
            cpu,
            direct_ml_recognizer,
            direct_ml_locator,
        })
    }

    pub(crate) fn detect_jpeg(&mut self, jpeg: &[u8]) -> Result<DetectionResult> {
        let image = decode_bounded_jpeg(jpeg)?;
        let located = if let Some(locator) = self.direct_ml_locator.as_mut() {
            locator.locate(&image)?
        } else {
            self.cpu.locator.locate(&image)?
        };
        let recognized = if let Some(recognizer) = self.direct_ml_recognizer.as_mut() {
            match recognizer.recognize_batch(&located.crops) {
                Ok(recognized) => recognized,
                Err(error) => {
                    eprintln!(
                        "DirectML text recognizer failed; using CPU for later requests: {error:#}"
                    );
                    self.direct_ml_recognizer = None;
                    recognize_cpu(&mut self.cpu.recognizer, &located.crops)?
                }
            }
        } else {
            recognize_cpu(&mut self.cpu.recognizer, &located.crops)?
        };
        Ok(located.complete(recognized))
    }
}

fn optional_acceleration<T>(loaded: Result<T>, label: &str) -> Option<T> {
    match loaded {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!("DirectML text {label} unavailable; using CPU: {error:#}");
            None
        }
    }
}

fn recognize_cpu(
    recognizer: &mut RecognizerCascade,
    crops: &[RgbImage],
) -> Result<Vec<crate::recognizer_cascade::RecognitionSet>> {
    let primary = recognizer.recognize_primary(crops)?;
    recognizer.recognize_alternatives(crops, primary)
}

impl DetectorBackend {
    fn load(detector: &Path, model_root: &Path, recognizer_catalog: &Path) -> Result<Self> {
        let model_root = model_root.to_path_buf();
        let recognizer_catalog = recognizer_catalog.to_path_buf();
        let recognizer_loader = std::thread::spawn(move || {
            RecognizerCascade::load(&model_root, &recognizer_catalog, Acceleration::Cpu, true)
        });
        let locator = TextLocator::load(detector, Acceleration::Cpu)?;
        let mut recognizer = recognizer_loader
            .join()
            .map_err(|_| anyhow::anyhow!("recognizer cascade loader thread panicked"))??;
        recognizer.warm_all()?;
        Ok(Self {
            locator,
            recognizer,
        })
    }
}

impl TextLocator {
    fn load(detector: &Path, acceleration: Acceleration) -> Result<Self> {
        let builder = Session::builder()?
            .with_memory_pattern(false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut builder = match acceleration {
            Acceleration::Cpu => builder,
            Acceleration::DirectMl => builder
                .with_optimization_level(GraphOptimizationLevel::Disable)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .with_execution_providers([ort::ep::DirectML::default().build()])
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        };
        let session = builder
            .commit_from_file(detector)
            .with_context(|| format!("load PaddleOCR detector '{}'", detector.display()))?;
        Ok(Self { session })
    }

    fn warm_up(&mut self) -> Result<()> {
        let tensor = Tensor::from_array((
            vec![1_usize, 3, WARMUP_SIDE, WARMUP_SIDE],
            vec![0.0_f32; 3 * WARMUP_SIDE * WARMUP_SIDE],
        ))?;
        self.session
            .run(ort::inputs![tensor])
            .context("warm the PaddleOCR detector")?;
        Ok(())
    }

    fn locate(&mut self, image: &RgbImage) -> Result<LocatedImage> {
        let image_width = image.width();
        let image_height = image.height();
        let prepared = PreparedImage::new(image)?;
        let tensor = Tensor::from_array((
            vec![
                1_usize,
                3,
                prepared.height as usize,
                prepared.width as usize,
            ],
            prepared.chw,
        ))?;
        let outputs = self.session.run(ort::inputs![tensor])?;
        let (shape, probabilities) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("read PaddleOCR probability map")?;
        let dimensions = shape.as_ref();
        if dimensions != [1, 1, prepared.height as i64, prepared.width as i64] {
            bail!("PaddleOCR returned an unexpected probability-map shape");
        }
        let regions = postprocess::extract_regions(
            probabilities,
            prepared.width,
            prepared.height,
            image_width,
            image_height,
        );
        let regions = crate::row_split::split(image, regions);
        let mut crops = Vec::with_capacity(regions.len());
        let mut crop_region_indices = Vec::with_capacity(regions.len());
        for (region_index, region) in regions.iter().enumerate() {
            let crop = image::imageops::crop_imm(
                image,
                region.left,
                region.top,
                region.right - region.left,
                region.bottom - region.top,
            )
            .to_image();
            crop_region_indices.push(region_index);
            crops.push(crop.clone());
            if needs_orientation_candidates(crop.width(), crop.height()) {
                crop_region_indices.extend([region_index, region_index]);
                crops.push(image::imageops::rotate90(&crop));
                crops.push(image::imageops::rotate270(&crop));
            }
        }
        Ok(LocatedImage {
            image_width,
            image_height,
            regions,
            crops,
            crop_region_indices,
        })
    }
}

impl LocatedImage {
    fn complete(
        mut self,
        recognized: Vec<crate::recognizer_cascade::RecognitionSet>,
    ) -> DetectionResult {
        let mut by_region = (0..self.regions.len()).map(|_| None).collect::<Vec<_>>();
        for (region_index, recognized) in self.crop_region_indices.into_iter().zip(recognized) {
            let slot = &mut by_region[region_index];
            *slot = Some(match slot.take() {
                Some(existing) => merge_recognition_sets(existing, recognized),
                None => recognized,
            });
        }
        for (region_index, region) in self.regions.iter_mut().enumerate() {
            let Some(recognized) = by_region[region_index].take() else {
                continue;
            };
            apply_recognition(region, recognized);
        }
        DetectionResult {
            image_width: self.image_width,
            image_height: self.image_height,
            regions: self.regions,
        }
    }
}

fn apply_recognition(
    region: &mut DetectedRegion,
    recognized: crate::recognizer_cascade::RecognitionSet,
) {
    let primary = normalize_recognition(recognized.primary);
    let mut seen = std::collections::HashSet::new();
    seen.insert(primary.text.clone());
    region.text = primary.text;
    region.text_confidence = primary.confidence;
    region.alternatives = recognized
        .alternatives
        .into_iter()
        .map(normalize_recognition)
        .filter(|candidate| !candidate.text.is_empty() && seen.insert(candidate.text.clone()))
        .take(MAX_RECOGNITION_ALTERNATIVES)
        .map(|candidate| RecognitionAlternative {
            text: candidate.text,
            confidence: candidate.confidence,
        })
        .collect();
}

fn normalize_recognition(
    mut recognition: crate::recognizer::Recognition,
) -> crate::recognizer::Recognition {
    truncate_utf8(&mut recognition.text, MAX_REGION_TEXT_BYTES);
    recognition.confidence = if recognition.confidence.is_finite() {
        recognition.confidence.clamp(0.0, 1.0)
    } else {
        0.0
    };
    recognition
}

fn truncate_utf8(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
}

fn needs_orientation_candidates(width: u32, height: u32) -> bool {
    height > width.saturating_mul(3) / 2
}

fn merge_recognition_sets(
    left: crate::recognizer_cascade::RecognitionSet,
    right: crate::recognizer_cascade::RecognitionSet,
) -> crate::recognizer_cascade::RecognitionSet {
    let mut candidates = std::iter::once(left.primary)
        .chain(left.alternatives)
        .chain(std::iter::once(right.primary))
        .chain(right.alternatives)
        .filter(|candidate| !candidate.text.trim().is_empty())
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        recognition_quality(right)
            .total_cmp(&recognition_quality(left))
            .then_with(|| right.text.chars().count().cmp(&left.text.chars().count()))
    });
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.text.clone()));
    let primary = candidates
        .first()
        .cloned()
        .unwrap_or(crate::recognizer::Recognition {
            text: String::new(),
            confidence: 0.0,
            script_evidence: Vec::new(),
            token_count: 0,
        });
    crate::recognizer_cascade::RecognitionSet {
        primary,
        alternatives: candidates,
    }
}

fn recognition_quality(recognition: &crate::recognizer::Recognition) -> f32 {
    let useful_characters = recognition
        .text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count()
        .clamp(1, 16) as f32;
    recognition.confidence.max(0.0) * useful_characters.sqrt()
}

struct PreparedImage {
    width: u32,
    height: u32,
    chw: Vec<f32>,
}

impl PreparedImage {
    fn new(source: &RgbImage) -> Result<Self> {
        let (width, height) = inference_size(source.width(), source.height());
        let resized = if (width, height) == source.dimensions() {
            source.clone()
        } else {
            image::imageops::resize(source, width, height, FilterType::Triangle)
        };
        let plane = (width as usize)
            .checked_mul(height as usize)
            .context("detector tensor size overflow")?;
        let mut chw = vec![0.0_f32; plane * 3];
        let mean = [0.485_f32, 0.456, 0.406];
        let std = [0.229_f32, 0.224, 0.225];
        for (index, pixel) in resized.pixels().enumerate() {
            let bgr = [pixel[2], pixel[1], pixel[0]];
            for channel in 0..3 {
                chw[channel * plane + index] =
                    (f32::from(bgr[channel]) / 255.0 - mean[channel]) / std[channel];
            }
        }
        Ok(Self { width, height, chw })
    }
}

fn decode_bounded_jpeg(jpeg: &[u8]) -> Result<RgbImage> {
    let mut reader = ImageReader::new(Cursor::new(jpeg))
        .with_guessed_format()
        .context("identify detector image")?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4);
    reader.limits(limits);
    let image = reader.decode().context("decode detector JPEG")?;
    let pixels = u64::from(image.width()) * u64::from(image.height());
    if pixels == 0 || pixels > MAX_IMAGE_PIXELS {
        bail!("detector image dimensions exceed the safety limit");
    }
    Ok(image.to_rgb8())
}

fn inference_size(width: u32, height: u32) -> (u32, u32) {
    let ratio = if width.max(height) > INFERENCE_LONG_SIDE {
        f64::from(INFERENCE_LONG_SIDE) / f64::from(width.max(height))
    } else {
        1.0
    };
    let scaled_width = (f64::from(width) * ratio) / 32.0;
    let scaled_height = (f64::from(height) * ratio) / 32.0;
    let width = (scaled_width.round_ties_even() as u32 * 32).max(32);
    let height = (scaled_height.round_ties_even() as u32 * 32).max(32);
    (width, height)
}

#[cfg(test)]
mod tests {
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
                regions: vec![region],
            },
        )
        .expect("normalized recognition output must satisfy the wire contract");
    }
}
