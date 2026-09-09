use anyhow::{Result, bail};
use image::{RgbImage, imageops::FilterType};
use std::ops::Range;

pub(super) const HEIGHT: usize = 48;
pub(super) const MAX_WIDTH: usize = 3200;
const CONTEXT: usize = 256;
const CORE: usize = MAX_WIDTH - 2 * CONTEXT;

pub(super) struct Tile {
    pub image: RgbImage,
    pub owned: Range<usize>,
}

pub(super) fn prepare(crop: &RgbImage) -> Result<Vec<Tile>> {
    let width = (HEIGHT * crop.width() as usize).div_ceil(crop.height() as usize);
    if width == 0 || width * HEIGHT > 40_000_000 {
        bail!("normalized reader line exceeds pixel budget");
    }
    let line = image::imageops::resize(crop, width as u32, HEIGHT as u32, FilterType::Triangle);
    if width <= MAX_WIDTH {
        return Ok(vec![Tile {
            image: line,
            owned: 0..width,
        }]);
    }
    Ok(ranges(width)
        .into_iter()
        .map(|(window, owned)| Tile {
            image: image::imageops::crop_imm(
                &line,
                window.start as u32,
                0,
                (window.end - window.start) as u32,
                HEIGHT as u32,
            )
            .to_image(),
            owned: owned.start - window.start..owned.end - window.start,
        })
        .collect())
}

fn ranges(width: usize) -> Vec<(Range<usize>, Range<usize>)> {
    if width <= MAX_WIDTH {
        return vec![(0..width, 0..width)];
    }
    (0..width)
        .step_by(CORE)
        .map(|start| {
            let end = (start + CORE).min(width);
            (
                start.saturating_sub(CONTEXT)..(end + CONTEXT).min(width),
                start..end,
            )
        })
        .collect()
}

impl Tile {
    pub fn owns_step(&self, step: usize, steps: usize, padded_width: usize) -> bool {
        if self.owned == (0..self.image.width() as usize) {
            return true;
        }
        // Keep a single spatial owner per timestep. Context supplies boundary
        // glyph evidence; it is never appended twice or text-deduplicated.
        let center = (2 * step + 1) * padded_width;
        center >= 2 * self.owned.start * steps && center < 2 * self.owned.end * steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_windows_own_every_pixel_once() {
        for width in [1, 3200, 3201, 4029, 65536, 393216] {
            let mut next = 0;
            for (window, owned) in ranges(width) {
                assert_eq!(owned.start, next);
                assert!(window.start <= owned.start && window.end >= owned.end);
                assert!(window.end - window.start <= MAX_WIDTH);
                next = owned.end;
            }
            assert_eq!(next, width);
        }
    }

    #[test]
    fn padding_and_context_have_no_output_ownership() {
        let tile = Tile {
            image: RgbImage::new(16, 48),
            owned: 4..12,
        };
        let owned = (0..8)
            .filter(|&step| tile.owns_step(step, 8, 32))
            .collect::<Vec<_>>();
        assert_eq!(owned, [1, 2]);
    }
}
