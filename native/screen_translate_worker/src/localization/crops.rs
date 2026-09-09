use super::boxes::distance;
use anyhow::{Context, Result, bail};
use image::{Rgb, RgbImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};
use sgt_screen_text_detector_protocol::stream::Region;

pub(crate) fn crop(image: &RgbImage, region: &Region) -> Result<RgbImage> {
    let q = region.quad;
    let width = distance(q[0], q[1])
        .max(distance(q[3], q[2]))
        .round()
        .max(1.0) as u32;
    let height = distance(q[0], q[3])
        .max(distance(q[1], q[2]))
        .round()
        .max(1.0) as u32;
    if u64::from(width) * u64::from(height) > 16_000_000 {
        bail!("reader crop exceeds pixel budget");
    }
    let projection = Projection::from_control_points(
        q.map(|p| (p[0], p[1])),
        [
            (0.0, 0.0),
            ((width - 1) as f32, 0.0),
            ((width - 1) as f32, (height - 1) as f32),
            (0.0, (height - 1) as f32),
        ],
    )
    .context("degenerate reader crop geometry")?;
    let mut crop = RgbImage::new(width, height);
    warp_into(
        image,
        &projection,
        Interpolation::Bilinear,
        Rgb([255, 255, 255]),
        &mut crop,
    );
    Ok(normalize_line(crop))
}

fn normalize_line(crop: RgbImage) -> RgbImage {
    // The CTC reader consumes left-to-right strips. Rotate tall rectified
    // regions counterclockwise without changing their source-space geometry.
    if u64::from(crop.height()) * 2 >= u64::from(crop.width()) * 3 {
        image::imageops::rotate270(&crop)
    } else {
        crop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tall_strip_rotates_counterclockwise_without_losing_pixels() {
        let source = RgbImage::from_fn(2, 6, |x, y| Rgb([x as u8, y as u8, 0]));
        let normalized = normalize_line(source.clone());
        assert_eq!(normalized.dimensions(), (6, 2));
        for (x, y, pixel) in source.enumerate_pixels() {
            assert_eq!(normalized.get_pixel(y, source.width() - 1 - x), pixel);
        }
    }

    #[test]
    fn horizontal_and_square_crops_keep_their_pixels() {
        for (width, height) in [(12, 3), (3, 3), (4, 5)] {
            let source = RgbImage::from_fn(width, height, |x, y| Rgb([x as u8, y as u8, 0]));
            assert_eq!(normalize_line(source.clone()), source);
        }
    }
}
