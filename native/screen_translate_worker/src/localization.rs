use anyhow::{Context, Result, bail};
use image::{ImageReader, Limits, RgbImage};
use ort::{session::Session, session::builder::GraphOptimizationLevel, value::Tensor};
use sgt_screen_text_detector_protocol::stream::Region;
use std::io::Cursor;
use std::path::Path;

mod boxes;
mod crops;
pub(crate) use crops::crop;

pub(crate) struct Locator {
    session: Session,
}

impl Locator {
    pub(crate) fn load(runtime: &Path, model: &Path) -> Result<Self> {
        if !ort::init_from(runtime.join("onnxruntime.dll"))?.commit() {
            bail!("ONNX Runtime was initialized before the worker handshake");
        }
        let session = Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_inter_threads(1)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_memory_pattern(false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_parallel_execution(false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .commit_from_file(model)
            .context("load text locator on DirectML")?;
        let mut locator = Self { session };
        for (width, height) in [(320, 320), (960, 544), (1600, 896)] {
            let input =
                Tensor::from_array((vec![1, 3, height, width], vec![0.0_f32; 3 * width * height]))?;
            locator
                .session
                .run(ort::inputs![input])
                .context("warm locator shapes")?;
        }
        Ok(locator)
    }

    pub(crate) fn locate(&mut self, image: &RgbImage) -> Result<Vec<Region>> {
        let (width, height) = inference_size(image.width(), image.height());
        let resized =
            image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle);
        let plane = width as usize * height as usize;
        let mut chw = vec![0.0_f32; 3 * plane];
        for (index, pixel) in resized.pixels().enumerate() {
            for channel in 0..3 {
                chw[channel * plane + index] = (f32::from(pixel[2 - channel]) / 255.0
                    - [0.485, 0.456, 0.406][channel])
                    / [0.229, 0.224, 0.225][channel];
            }
        }
        let input = Tensor::from_array((vec![1, 3, height as usize, width as usize], chw))?;
        let outputs = self.session.run(ort::inputs![input])?;
        let (shape, probabilities) = outputs[0].try_extract_tensor::<f32>()?;
        if shape.as_ref() != [1, 1, height as i64, width as i64] {
            bail!("locator returned an unexpected probability map");
        }
        boxes::extract(probabilities, width, height, image.width(), image.height())
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<RgbImage> {
    let mut decoder = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(160_000_000);
    decoder.limits(limits);
    let image = decoder.decode()?.to_rgb8();
    if image.width() == 0
        || image.height() == 0
        || u64::from(image.width()) * u64::from(image.height()) > 40_000_000
    {
        bail!("capture exceeds the image safety limit");
    }
    Ok(image)
}

fn inference_size(width: u32, height: u32) -> (u32, u32) {
    let ratio = (1600.0 / f64::from(width.max(height))).min(1.0);
    let aligned = |side| ((f64::from(side) * ratio / 32.0).round_ties_even() as u32 * 32).max(32);
    (aligned(width), aligned(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preprocessing_is_bounded_and_aligned() {
        for (w, h) in [(1, 1), (1920, 1080), (8192, 8192), (8192, 12)] {
            let (w, h) = inference_size(w, h);
            assert!(w <= 1600 && h <= 1600 && w % 32 == 0 && h % 32 == 0);
        }
    }
}
