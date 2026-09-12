use super::boxes::distance;
use anyhow::{Context, Result, bail};
use image::{Rgb, RgbImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};
use sgt_screen_text_detector_protocol::stream::Region;

pub(crate) fn crop(image: &RgbImage, region: &Region, regions: &[Region]) -> Result<RgbImage> {
    let q = reading_quad(region, regions);
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

fn reading_quad(region: &Region, regions: &[Region]) -> [[f32; 2]; 4] {
    let q = region.quad;
    let width = distance(q[0], q[1]);
    let height = distance(q[0], q[3]);
    if width <= 0.0 || height <= 0.0 {
        return q;
    }
    let ux = [(q[1][0] - q[0][0]) / width, (q[1][1] - q[0][1]) / width];
    let uy = [(q[3][0] - q[0][0]) / height, (q[3][1] - q[0][1]) / height];
    let project =
        |p: [f32; 2], axis: [f32; 2]| (p[0] - q[0][0]) * axis[0] + (p[1] - q[0][1]) * axis[1];
    let mut top = [0.0_f32; 2];
    let mut bottom = [height; 2];
    for other in regions {
        if other.id == region.id {
            continue;
        }
        let edge = [
            other.quad[1][0] - other.quad[0][0],
            other.quad[1][1] - other.quad[0][1],
        ];
        let length = distance(other.quad[0], other.quad[1]);
        if length <= 0.0 || (edge[0] * ux[0] + edge[1] * ux[1]).abs() < length * 0.98 {
            continue;
        }
        let midpoint = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let ends = [
            midpoint(other.quad[0], other.quad[3]),
            midpoint(other.quad[1], other.quad[2]),
        ];
        let mut xs = ends.map(|p| project(p, ux));
        let mut ys = ends.map(|p| project(p, uy));
        if xs[0] > xs[1] {
            xs.swap(0, 1);
            ys.swap(0, 1);
        }
        let [left, right] = xs;
        // A projected axis-aligned extent grows with line length and tilt;
        // it is not the text's cross-axis size. Use its own short edges.
        let other_height =
            (distance(other.quad[0], other.quad[3]) + distance(other.quad[1], other.quad[2])) * 0.5;
        if other_height <= 0.0
            || right <= left
            || height.max(other_height) > height.min(other_height) * 1.6
            || right.min(width) - left.max(0.0) < width.min(right - left) * 0.6
        {
            continue;
        }
        let centers = [0.0, width].map(|x| ys[0] + (ys[1] - ys[0]) * (x - left) / (right - left));
        let gaps = centers.map(|center| center - height * 0.5);
        if gaps[0] * gaps[1] <= 0.0
            || gaps
                .iter()
                .any(|gap| gap.abs() < height.min(other_height) * 0.25)
        {
            continue;
        }
        // Unclipping gives recognition context, not ownership of a neighboring
        // line's glyphs. Bisect the centerlines at both ends, retaining tilt.
        // Crossing or near-coincident centerlines never clip one another.
        for (side, center) in centers.into_iter().enumerate() {
            let boundary = (height * 0.5 + center) * 0.5;
            if gaps[side] < 0.0 {
                top[side] = top[side].max(boundary);
            } else {
                bottom[side] = bottom[side].min(boundary);
            }
        }
    }
    if (0..2).any(|side| bottom[side] - top[side] < 1.0) {
        return q;
    }
    std::array::from_fn(|i| {
        let side = usize::from(i == 1 || i == 2);
        let offset = if i < 2 {
            top[side]
        } else {
            bottom[side] - height
        };
        [q[i][0] + uy[0] * offset, q[i][1] + uy[1] * offset]
    })
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
    fn parallel_overlapping_strips_have_separate_reading_ownership() {
        let make = |id, y| Region {
            id,
            confidence: 1.0,
            quad: [[0.0, y], [40.0, y], [40.0, y + 20.0], [0.0, y + 20.0]],
        };
        let regions = [make(1, 0.0), make(2, 16.0)];
        let first = reading_quad(&regions[0], &regions);
        let second = reading_quad(&regions[1], &regions);
        assert_eq!(first[2][1], 18.0);
        assert_eq!(second[0][1], 18.0);
        assert_eq!(regions[0].quad[2][1], 20.0);
        let isolated = [make(1, 0.0), make(2, 50.0)];
        assert_eq!(reading_quad(&isolated[0], &isolated), isolated[0].quad);
    }

    #[test]
    fn tilted_neighbors_use_their_centerline_not_projected_height() {
        let sources = [
            Region {
                id: 1,
                confidence: 1.0,
                quad: [[0.0, 0.0], [1000.0, 0.0], [1000.0, 20.0], [0.0, 20.0]],
            },
            Region {
                id: 2,
                confidence: 1.0,
                quad: [[0.0, 6.0], [1000.0, 22.0], [1000.0, 42.0], [0.0, 26.0]],
            },
        ];
        let separated = reading_quad(&sources[0], &sources);
        assert_eq!(separated[3][1], 13.0);
        assert_eq!(separated[2][1], 20.0);
        // A global rotation must rotate the same crop, not change ownership.
        let rotate = |p: [f32; 2]| [p[0] * 0.8 - p[1] * 0.6, p[0] * 0.6 + p[1] * 0.8];
        let rotated = sources.map(|r| Region {
            quad: r.quad.map(rotate),
            ..r
        });
        let actual = reading_quad(&rotated[0], &rotated);
        for (actual, expected) in actual.into_iter().zip(separated.map(rotate)) {
            assert!(distance(actual, expected) < 0.001);
        }
    }

    #[test]
    fn crossing_and_coincident_lines_do_not_cut_each_other() {
        let source = Region {
            id: 1,
            confidence: 1.0,
            quad: [[0.0, 0.0], [1000.0, 0.0], [1000.0, 20.0], [0.0, 20.0]],
        };
        for quad in [
            source.quad,
            [[0.0, -8.0], [1000.0, 8.0], [1000.0, 28.0], [0.0, 12.0]],
        ] {
            let other = Region {
                id: 2,
                quad,
                confidence: 1.0,
            };
            assert_eq!(reading_quad(&source, &[source.clone(), other]), source.quad);
        }
    }

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
