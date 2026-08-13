use std::io::Cursor;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::JoinHandle;

use anyhow::{Context, Result, bail};
use image::imageops::FilterType;
use image::{ImageReader, Limits, RgbImage};
use ort::session::Session;
use ort::value::Tensor;
use sgt_screen_text_detector_protocol::{DetectedRegion, RecognitionAlternative};

use crate::postprocess;
use crate::recognizer::Acceleration;
use crate::recognizer_cascade::RecognizerCascade;

const MAX_IMAGE_SIDE: u32 = 8_192;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const INFERENCE_LONG_SIDE: u32 = 1_600;
const DIRECT_ML_LOCATOR_MIN_PIXELS: u64 = 600_000;
const WARMUP_SIDE: usize = 320;

pub(crate) struct DetectionResult {
    pub(crate) image_width: u32,
    pub(crate) image_height: u32,
    pub(crate) regions: Vec<DetectedRegion>,
}

pub(crate) struct TextDetector {
    cpu: DetectorBackend,
    direct_ml_recognizer: Option<RecognizerCascade>,
    direct_ml_recognizer_receiver: Option<Receiver<Result<RecognizerCascade>>>,
    direct_ml_locator: Option<TextLocator>,
    direct_ml_locator_receiver: Option<Receiver<Result<TextLocator>>>,
    _direct_ml_recognizer_loader: JoinHandle<()>,
    _direct_ml_locator_loader: JoinHandle<()>,
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
}

impl TextDetector {
    pub(crate) fn load(
        cpu_detector: &Path,
        direct_ml_detector: &Path,
        model_root: &Path,
        recognizer_catalog: &Path,
    ) -> Result<Self> {
        let cpu = DetectorBackend::load(cpu_detector, model_root, recognizer_catalog)?;
        let model_root = model_root.to_path_buf();
        let recognizer_catalog = recognizer_catalog.to_path_buf();
        let (recognizer_sender, recognizer_receiver) = std::sync::mpsc::sync_channel(1);
        let recognizer_loader = std::thread::spawn(move || {
            let loaded = RecognizerCascade::load(
                &model_root,
                &recognizer_catalog,
                Acceleration::DirectMl,
                true,
            )
            .and_then(|mut cascade| {
                cascade.warm_all()?;
                Ok(cascade)
            });
            let _ = recognizer_sender.send(loaded);
        });
        let detector = direct_ml_detector.to_path_buf();
        let (locator_sender, locator_receiver) = std::sync::mpsc::sync_channel(1);
        let locator_loader = std::thread::spawn(move || {
            let loaded =
                TextLocator::load(&detector, Acceleration::DirectMl).and_then(|mut locator| {
                    locator.warm_up()?;
                    Ok(locator)
                });
            let _ = locator_sender.send(loaded);
        });
        let mut loaded = Self {
            cpu,
            direct_ml_recognizer: None,
            direct_ml_recognizer_receiver: Some(recognizer_receiver),
            direct_ml_locator: None,
            direct_ml_locator_receiver: Some(locator_receiver),
            _direct_ml_recognizer_loader: recognizer_loader,
            _direct_ml_locator_loader: locator_loader,
        };
        loaded.finish_acceleration_setup();
        Ok(loaded)
    }

    fn finish_acceleration_setup(&mut self) {
        receive_optional(
            &mut self.direct_ml_recognizer,
            &mut self.direct_ml_recognizer_receiver,
            true,
            "recognizer",
        );
        receive_optional(
            &mut self.direct_ml_locator,
            &mut self.direct_ml_locator_receiver,
            true,
            "locator",
        );
    }

    pub(crate) fn detect_jpeg(&mut self, jpeg: &[u8]) -> Result<DetectionResult> {
        receive_optional(
            &mut self.direct_ml_recognizer,
            &mut self.direct_ml_recognizer_receiver,
            false,
            "recognizer",
        );
        receive_optional(
            &mut self.direct_ml_locator,
            &mut self.direct_ml_locator_receiver,
            false,
            "locator",
        );
        let image = decode_bounded_jpeg(jpeg)?;
        let pixels = u64::from(image.width()) * u64::from(image.height());
        let prefer_direct_ml = pixels >= DIRECT_ML_LOCATOR_MIN_PIXELS;
        if prefer_direct_ml && self.direct_ml_locator.is_none() {
            receive_optional(
                &mut self.direct_ml_locator,
                &mut self.direct_ml_locator_receiver,
                true,
                "locator",
            );
        }
        let located = if let Some(locator) = self.direct_ml_locator.as_mut() {
            locator.locate(&image)?
        } else {
            self.cpu.locator.locate(&image)?
        };
        if prefer_direct_ml && self.direct_ml_recognizer.is_none() {
            receive_optional(
                &mut self.direct_ml_recognizer,
                &mut self.direct_ml_recognizer_receiver,
                true,
                "recognizer",
            );
        }
        if let Some(direct_ml) = self
            .direct_ml_recognizer
            .as_mut()
            .filter(|_| prefer_direct_ml)
        {
            let recognized = direct_ml.recognize_batch(&located.crops)?;
            return Ok(located.complete(recognized));
        }
        let recognized = if let Some(direct_ml) = self.direct_ml_recognizer.as_mut() {
            direct_ml.recognize_batch(&located.crops)?
        } else {
            let primary = self.cpu.recognizer.recognize_primary(&located.crops)?;
            if RecognizerCascade::batch_needs_alternatives(&primary) {
                receive_optional(
                    &mut self.direct_ml_recognizer,
                    &mut self.direct_ml_recognizer_receiver,
                    true,
                    "recognizer",
                );
            }
            if let Some(direct_ml) = self.direct_ml_recognizer.as_mut() {
                direct_ml.recognize_batch(&located.crops)?
            } else {
                self.cpu
                    .recognizer
                    .recognize_alternatives(&located.crops, primary)?
            }
        };
        Ok(located.complete(recognized))
    }
}

fn receive_optional<T>(
    target: &mut Option<T>,
    receiver_slot: &mut Option<Receiver<Result<T>>>,
    wait: bool,
    label: &str,
) {
    let Some(receiver) = receiver_slot.take() else {
        return;
    };
    let received = if wait {
        receiver.recv().ok()
    } else {
        match receiver.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => {
                *receiver_slot = Some(receiver);
                return;
            }
            Err(TryRecvError::Disconnected) => None,
        }
    };
    match received {
        Some(Ok(value)) => *target = Some(value),
        Some(Err(error)) => eprintln!("DirectML text {label} unavailable; using CPU: {error:#}"),
        None => {}
    }
}

impl DetectorBackend {
    fn load(detector: &Path, model_root: &Path, recognizer_catalog: &Path) -> Result<Self> {
        let model_root = model_root.to_path_buf();
        let recognizer_catalog = recognizer_catalog.to_path_buf();
        let recognizer_loader = std::thread::spawn(move || {
            RecognizerCascade::load(&model_root, &recognizer_catalog, Acceleration::Cpu, false)
        });
        let locator = TextLocator::load(detector, Acceleration::Cpu)?;
        let recognizer = recognizer_loader
            .join()
            .map_err(|_| anyhow::anyhow!("recognizer cascade loader thread panicked"))??;
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
        let crops = regions
            .iter()
            .map(|region| {
                image::imageops::crop_imm(
                    image,
                    region.left,
                    region.top,
                    region.right - region.left,
                    region.bottom - region.top,
                )
                .to_image()
            })
            .collect::<Vec<_>>();
        Ok(LocatedImage {
            image_width,
            image_height,
            regions,
            crops,
        })
    }
}

impl LocatedImage {
    fn complete(
        mut self,
        recognized: Vec<crate::recognizer_cascade::RecognitionSet>,
    ) -> DetectionResult {
        for (region, recognized) in self.regions.iter_mut().zip(recognized) {
            region.text = recognized.primary.text;
            region.text_confidence = recognized.primary.confidence;
            region.alternatives = recognized
                .alternatives
                .into_iter()
                .map(|candidate| RecognitionAlternative {
                    text: candidate.text,
                    confidence: candidate.confidence,
                })
                .collect();
        }
        DetectionResult {
            image_width: self.image_width,
            image_height: self.image_height,
            regions: self.regions,
        }
    }
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
}
