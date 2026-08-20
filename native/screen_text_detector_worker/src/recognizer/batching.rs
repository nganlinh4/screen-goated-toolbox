use anyhow::{Context, Result, bail};
use image::RgbImage;
use image::imageops::FilterType;

use super::{INPUT_HEIGHT, MIN_INPUT_WIDTH};

const MAX_BATCH_SIZE: usize = 16;
const MAX_WIDTH_RATIO: u32 = 2;
const MAX_LOGIT_WORK: usize = 24 * 1024 * 1024 / size_of::<f32>();

pub(super) struct PreparedTextLine {
    pub(super) width: u32,
    pub(super) chw: Vec<f32>,
}

pub(super) fn prepare(source: &RgbImage, max_input_width: u32) -> Result<PreparedTextLine> {
    if source.width() == 0 || source.height() == 0 {
        bail!("recognizer crop is empty");
    }
    let scaled =
        (source.width() as f64 * f64::from(INPUT_HEIGHT) / source.height() as f64).ceil() as u32;
    let resized_width = scaled.clamp(1, max_input_width);
    let input_width = resized_width
        .div_ceil(32)
        .saturating_mul(32)
        .clamp(MIN_INPUT_WIDTH, max_input_width);
    let resized =
        image::imageops::resize(source, resized_width, INPUT_HEIGHT, FilterType::Triangle);
    let plane = input_width as usize * INPUT_HEIGHT as usize;
    let mut chw = vec![0.0_f32; plane * 3];
    for (y, row) in resized.rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let offset = y * input_width as usize + x;
            let bgr = [pixel[2], pixel[1], pixel[0]];
            for channel in 0..3 {
                chw[channel * plane + offset] = f32::from(bgr[channel]) / 127.5 - 1.0;
            }
        }
    }
    Ok(PreparedTextLine {
        width: input_width,
        chw,
    })
}

pub(super) fn recognition_batches(
    prepared: &[PreparedTextLine],
    class_count: usize,
) -> Vec<Vec<usize>> {
    let mut order = (0..prepared.len()).collect::<Vec<_>>();
    order.sort_unstable_by_key(|index| prepared[*index].width);
    let mut batches = Vec::new();
    let mut batch: Vec<usize> = Vec::new();
    for index in order {
        let minimum_width = batch
            .first()
            .map_or(prepared[index].width, |first| prepared[*first].width);
        let candidate_count = batch.len() + 1;
        let candidate_width = prepared[index].width as usize;
        let steps = candidate_width.div_ceil(8);
        let work = candidate_count
            .saturating_mul(steps)
            .saturating_mul(class_count);
        let exceeds_shape = prepared[index].width > minimum_width.saturating_mul(MAX_WIDTH_RATIO);
        if !batch.is_empty()
            && (candidate_count > MAX_BATCH_SIZE || work > MAX_LOGIT_WORK || exceeds_shape)
        {
            batches.push(std::mem::take(&mut batch));
        }
        batch.push(index);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

pub(super) fn batch_tensor(
    prepared: &[PreparedTextLine],
    indices: &[usize],
    batch_width: u32,
) -> Result<Vec<f32>> {
    let height = INPUT_HEIGHT as usize;
    let batch_plane = batch_width as usize * height;
    let mut tensor = vec![0.0_f32; indices.len() * batch_plane * 3];
    for (batch_index, source_index) in indices.iter().enumerate() {
        let source = prepared
            .get(*source_index)
            .context("recognizer batch index is invalid")?;
        let source_plane = source.width as usize * height;
        for channel in 0..3 {
            for row in 0..height {
                let source_start = channel * source_plane + row * source.width as usize;
                let target_start = batch_index * batch_plane * 3
                    + channel * batch_plane
                    + row * batch_width as usize;
                tensor[target_start..target_start + source.width as usize].copy_from_slice(
                    &source.chw[source_start..source_start + source.width as usize],
                );
            }
        }
    }
    Ok(tensor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(widths: &[u32]) -> Vec<PreparedTextLine> {
        widths
            .iter()
            .map(|width| PreparedTextLine {
                width: *width,
                chw: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn batches_preserve_every_input_without_excessive_width_padding() {
        let lines = prepared(&[640, 32, 96, 1_600, 320, 64, 1_024]);
        let batches = recognition_batches(&lines, 18_710);
        let mut indices = batches.iter().flatten().copied().collect::<Vec<_>>();
        indices.sort_unstable();
        assert_eq!(indices, (0..lines.len()).collect::<Vec<_>>());
        assert!(batches.iter().all(|batch| batch.len() <= MAX_BATCH_SIZE));
        assert!(batches.iter().all(|batch| {
            let first = lines[*batch.first().unwrap()].width;
            let last = lines[*batch.last().unwrap()].width;
            last <= first.saturating_mul(MAX_WIDTH_RATIO)
        }));
    }

    #[test]
    fn large_alphabets_reduce_batch_size_by_work_budget() {
        let lines = prepared(&[640; MAX_BATCH_SIZE]);
        let batches = recognition_batches(&lines, 18_710);
        assert!(batches.len() > 1);
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), lines.len());
    }
}
