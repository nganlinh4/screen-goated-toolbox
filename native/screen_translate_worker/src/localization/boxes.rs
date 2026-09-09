use anyhow::{Result, bail};
use image::{GrayImage, Luma};
use imageproc::{contours::find_contours, geometry::min_area_rect};
use sgt_screen_text_detector_protocol::{MAX_REGIONS, stream::Region};

pub(super) fn extract(
    map: &[f32],
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
) -> Result<Vec<Region>> {
    if map.len() != width as usize * height as usize || map.iter().any(|v| !v.is_finite()) {
        bail!("invalid locator probability map");
    }
    let mut bitmap = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            // A 2x2 dilation closes small gaps without re-running localization.
            let on = (y.saturating_sub(1)..=y).any(|py| {
                (x.saturating_sub(1)..=x).any(|px| map[(py * width + px) as usize] > 0.3)
            });
            bitmap.put_pixel(x, y, Luma([if on { 255 } else { 0 }]));
        }
    }
    let mut regions = Vec::new();
    for contour in find_contours::<i32>(&bitmap) {
        if contour.points.len() < 4 {
            continue;
        }
        let rect = min_area_rect(&contour.points);
        let mut points = rect.map(|p| [p.x as f32, p.y as f32]);
        order(&mut points);
        let w = distance(points[0], points[1]);
        let h = distance(points[0], points[3]);
        if w.min(h) < 3.0 {
            continue;
        }
        let confidence = score(map, width, height, &points);
        if confidence < 0.5 {
            continue;
        }
        let expand = w * h * 1.6 / (2.0 * (w + h));
        let ux = [
            (points[1][0] - points[0][0]) / w,
            (points[1][1] - points[0][1]) / w,
        ];
        let uy = [
            (points[3][0] - points[0][0]) / h,
            (points[3][1] - points[0][1]) / h,
        ];
        for (index, point) in points.iter_mut().enumerate() {
            let sx = if index == 0 || index == 3 { -1.0 } else { 1.0 };
            let sy = if index < 2 { -1.0 } else { 1.0 };
            point[0] = ((point[0] + expand * (sx * ux[0] + sy * uy[0])) / width as f32
                * source_width as f32)
                .round()
                .clamp(0.0, (source_width - 1) as f32);
            point[1] = ((point[1] + expand * (sx * ux[1] + sy * uy[1])) / height as f32
                * source_height as f32)
                .round()
                .clamp(0.0, (source_height - 1) as f32);
        }
        if distance(points[0], points[1]).min(distance(points[0], points[3])) <= 3.0 {
            continue;
        }
        if regions.len() == MAX_REGIONS {
            bail!("capture exceeds the region safety limit");
        }
        regions.push(Region {
            id: 0,
            quad: points,
            confidence,
        });
    }
    regions.sort_by(|a, b| {
        a.quad[0][1]
            .total_cmp(&b.quad[0][1])
            .then(a.quad[0][0].total_cmp(&b.quad[0][0]))
    });
    for (index, region) in regions.iter_mut().enumerate() {
        region.id = (index + 1) as u32;
    }
    Ok(regions)
}

fn order(points: &mut [[f32; 2]; 4]) {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    if points[0][1] > points[1][1] {
        points.swap(0, 1);
    }
    if points[2][1] > points[3][1] {
        points.swap(2, 3);
    }
    let [tl, bl, tr, br] = *points;
    *points = [tl, tr, br, bl];
}

pub(super) fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn score(map: &[f32], width: u32, height: u32, quad: &[[f32; 2]; 4]) -> f32 {
    let left = quad
        .iter()
        .map(|p| p[0].floor() as u32)
        .min()
        .unwrap()
        .min(width - 1);
    let right = quad
        .iter()
        .map(|p| p[0].ceil() as u32)
        .max()
        .unwrap()
        .min(width - 1);
    let top = quad
        .iter()
        .map(|p| p[1].floor() as u32)
        .min()
        .unwrap()
        .min(height - 1);
    let bottom = quad
        .iter()
        .map(|p| p[1].ceil() as u32)
        .max()
        .unwrap()
        .min(height - 1);
    let mut total = 0.0;
    let mut count = 0;
    for y in top..=bottom {
        for x in left..=right {
            let mut positive = false;
            let mut negative = false;
            for i in 0..4 {
                let a = quad[i];
                let b = quad[(i + 1) % 4];
                let cross = (b[0] - a[0]) * (y as f32 - a[1]) - (b[1] - a[1]) * (x as f32 - a[0]);
                positive |= cross > 0.01;
                negative |= cross < -0.01;
            }
            if !(positive && negative) {
                total += map[(y * width + x) as usize];
                count += 1;
            }
        }
    }
    total / (count.max(1) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_map_is_empty_and_regions_retain_source_coordinates() {
        assert!(
            extract(&vec![0.0; 64 * 64], 64, 64, 128, 128)
                .unwrap()
                .is_empty()
        );
        let mut map = vec![0.0; 64 * 64];
        for y in 20..30 {
            for x in 10..45 {
                map[y * 64 + x] = 1.0;
            }
        }
        let boxes = extract(&map, 64, 64, 128, 128).unwrap();
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].id, 1);
        assert!(boxes[0].quad[0][0] < 20.0 && boxes[0].quad[2][0] > 88.0);
    }
}
