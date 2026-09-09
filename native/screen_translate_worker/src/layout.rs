use anyhow::{Context, Result, bail};
use image::RgbImage;
use ort::{session::Session, value::Tensor};
use sgt_screen_text_detector_protocol::stream::{LayoutKind, LayoutRegion};
use std::path::Path;

pub(crate) struct Layout {
    session: Session,
}

impl Layout {
    pub(crate) fn load(model: &Path) -> Result<Self> {
        let session = Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_inter_threads(1)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_memory_pattern(false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_parallel_execution(false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .commit_from_file(model)
            .context("load advisory layout on DirectML")?;
        let mut layout = Self { session };
        layout.detect(&RgbImage::new(800, 800))?;
        Ok(layout)
    }

    pub(crate) fn detect(&mut self, image: &RgbImage) -> Result<Vec<LayoutRegion>> {
        let resized =
            image::imageops::resize(image, 800, 800, image::imageops::FilterType::CatmullRom);
        let mut values = vec![0.0_f32; 3 * 800 * 800];
        for (index, pixel) in resized.pixels().enumerate() {
            for channel in 0..3 {
                values[channel * 800 * 800 + index] = f32::from(pixel[channel]) / 255.0;
            }
        }
        let image_tensor = Tensor::from_array(([1, 3, 800, 800], values))?;
        let shape = Tensor::from_array(([1, 2], vec![800.0_f32, 800.0]))?;
        let scale = Tensor::from_array((
            [1, 2],
            vec![800.0 / image.height() as f32, 800.0 / image.width() as f32],
        ))?;
        let output = self.session.run(
            ort::inputs!["image" => image_tensor, "im_shape" => shape, "scale_factor" => scale],
        )?;
        let (shape, boxes) = output[0].try_extract_tensor::<f32>()?;
        if shape.len() != 2 || shape[1] != 7 {
            bail!("unexpected advisory layout shape");
        }
        let mut result = Vec::new();
        for row in boxes.as_chunks::<7>().0 {
            if row.iter().any(|value| !value.is_finite()) || row[1] < 0.5 {
                continue;
            }
            let bounds = [
                row[2].clamp(0.0, image.width() as f32),
                row[3].clamp(0.0, image.height() as f32),
                row[4].clamp(0.0, image.width() as f32),
                row[5].clamp(0.0, image.height() as f32),
            ];
            if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
                continue;
            }
            let kind = match row[0] as u32 {
                21 => LayoutKind::Table,
                23 => LayoutKind::VerticalText,
                0 | 2 | 4 | 6 | 7 | 8 | 10 | 12 | 17 | 18 | 19 | 22 | 24 => LayoutKind::Text,
                _ => LayoutKind::Other,
            };
            result.push(LayoutRegion { bounds, kind });
        }
        Ok(result)
    }
}
