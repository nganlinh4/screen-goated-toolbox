use super::super::contract::DetectedTextRegion;
use super::super::geometry::{PixelRegion, normalized_region};
use sgt_screen_text_detector_protocol::stream::{LayoutKind, LayoutRegion};

struct Feature {
    box_: PixelRegion,
    em: f32,
    owner: Option<usize>,
    table: Option<usize>,
    independent: bool,
}

pub(super) fn partition(
    image: &image::RgbaImage,
    sources: &[DetectedTextRegion],
    layout: &[LayoutRegion],
) -> Vec<Vec<usize>> {
    let mut features = sources
        .iter()
        .map(|source| {
            let box_ = normalized_region(source.bounds, image.width(), image.height());
            let cx = box_.x as f32 + box_.width as f32 / 2.0;
            let cy = box_.y as f32 + box_.height as f32 / 2.0;
            let containing = |kind: bool| {
                layout
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        (if kind {
                            r.kind == LayoutKind::Table
                        } else {
                            matches!(r.kind, LayoutKind::Text | LayoutKind::VerticalText)
                        }) && cx >= r.bounds[0]
                            && cx <= r.bounds[2]
                            && cy >= r.bounds[1]
                            && cy <= r.bounds[3]
                    })
                    .min_by(|(_, a), (_, b)| {
                        let area_a = (a.bounds[2] - a.bounds[0]) * (a.bounds[3] - a.bounds[1]);
                        let area_b = (b.bounds[2] - b.bounds[0]) * (b.bounds[3] - b.bounds[1]);
                        area_a.total_cmp(&area_b)
                    })
                    .map(|(i, _)| i)
            };
            Feature {
                independent: false,
                box_,
                owner: containing(false),
                table: containing(true),
                em: super::super::text_metrics::preferred_font_size(
                    image,
                    std::iter::once((
                        box_,
                        source
                            .appearance
                            .map(|a| (a.background_rgb, a.background_confidence)),
                    )),
                ),
            }
        })
        .collect::<Vec<_>>();
    let row_cells = features
        .iter()
        .map(|feature| super::rows::Cell {
            rect: feature.box_,
            paragraph: feature.owner.is_some(),
            table: feature.table.is_some(),
        })
        .collect::<Vec<_>>();
    let independent = super::rows::independent(&row_cells, |a, b| {
        continuous_surface(
            image,
            features[a].box_,
            features[b].box_,
            sources[a].appearance,
            sources[b].appearance,
        )
    });
    for (feature, independent) in features.iter_mut().zip(independent) {
        feature.independent = independent;
    }
    let mut order = (0..sources.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| (features[i].box_.y, features[i].box_.x));
    let mut used = vec![false; sources.len()];
    let mut result = Vec::new();
    for &start in &order {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut members = vec![start];
        loop {
            let current = *members.last().unwrap();
            let next = order
                .iter()
                .copied()
                .filter(|&i| !used[i])
                .filter(|&i| {
                    compatible(
                        image,
                        &features[current],
                        &features[i],
                        &sources[current],
                        &sources[i],
                    )
                })
                .filter(|&i| !encloses_other(&members, i, &features))
                .min_by_key(|&i| {
                    features[current].box_.y.abs_diff(features[i].box_.y) * 4
                        + features[current].box_.x.abs_diff(features[i].box_.x)
                });
            let Some(next) = next else {
                break;
            };
            used[next] = true;
            members.push(next);
        }
        // Vertical source columns conventionally read from right to left.
        if members.iter().all(|&i| vertical(features[i].box_)) {
            members.sort_by_key(|&i| (std::cmp::Reverse(features[i].box_.x), features[i].box_.y));
        }
        result.push(members);
    }
    result
}

fn continuous_surface(
    image: &image::RgbaImage,
    a: PixelRegion,
    b: PixelRegion,
    left: Option<super::super::appearance::VisualSignature>,
    right: Option<super::super::appearance::VisualSignature>,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    if left.background_confidence < 45
        || right.background_confidence < 45
        || distance(left.background_rgb, right.background_rgb) > 24
    {
        return false;
    }
    let start = a.x + a.width;
    let end = b.x;
    let top = a.y.max(b.y);
    let bottom = (a.y + a.height).min(b.y + b.height);
    if end <= start || bottom <= top {
        return false;
    }
    let mut matching = 0;
    // A fixed sample budget admits thin rules but rejects panel imagery and
    // disconnected balloons. No work scales with gutter area or image pixels.
    for row in 0..3 {
        let y = top + (bottom - top - 1) * (row * 2 + 1) / 6;
        for column in 0..32 {
            let x = start + (end - start - 1) * (column * 2 + 1) / 64;
            let p = image.get_pixel(x, y).0;
            matching += usize::from(distance([p[0], p[1], p[2]], left.background_rgb) <= 32);
        }
    }
    matching >= 80
}

fn vertical(a: PixelRegion) -> bool {
    a.height > a.width.saturating_mul(3) / 2
}
fn cross_size(a: PixelRegion) -> u32 {
    if vertical(a) { a.width } else { a.height }
}
fn overlap(a: u32, length: u32, b: u32, other: u32) -> u32 {
    (a + length).min(b + other).saturating_sub(a.max(b))
}

fn compatible(
    image: &image::RgbaImage,
    a: &Feature,
    b: &Feature,
    left: &DetectedTextRegion,
    right: &DetectedTextRegion,
) -> bool {
    let shared = a.owner.is_some() && a.owner == b.owner;
    let size_ratio = if shared { 1.6 } else { 1.25 };
    // Ink height changes with ascenders and descenders even at one font size.
    // Require the detector's cross-axis extent to corroborate a size boundary.
    if a.independent
        || b.independent
        || a.table != b.table
        || a.owner.zip(b.owner).is_some_and(|(x, y)| x != y)
        || (a.em.max(b.em) > a.em.min(b.em) * size_ratio
            && cross_size(a.box_).max(cross_size(b.box_)) as f32
                > cross_size(a.box_).min(cross_size(b.box_)) as f32 * size_ratio)
    {
        return false;
    }
    if let (Some(x), Some(y)) = (left.appearance, right.appearance) {
        if x.background_confidence >= 45
            && y.background_confidence >= 45
            && distance(x.background_rgb, y.background_rgb) > 40
        {
            return false;
        }
        if x.foreground_confidence >= 3
            && y.foreground_confidence >= 3
            && let (Some(fx), Some(fy)) = (x.foreground_rgb, y.foreground_rgb)
            && distance(fx, fy) > 48
            && (!same_ink_direction(fx, x.background_rgb, fy, y.background_rgb)
                || (x.background_confidence >= 45
                    && y.background_confidence >= 45
                    && overlap(a.box_.y, a.box_.height, b.box_.y, b.box_.height) * 2
                        < a.box_.height.min(b.box_.height)
                    && contrast_boundary(fx, x.background_rgb, fy, y.background_rgb)))
        {
            return false;
        }
    }
    let (x, y) = (a.box_, b.box_);
    if vertical(x) || vertical(y) {
        return shared
            && vertical(x)
            && vertical(y)
            && a.table.is_none()
            && x.y.abs_diff(y.y) as f32 <= a.em
            && x.x.abs_diff(y.x) <= x.width.max(y.width) * 2;
    }
    let same_line = overlap(x.y, x.height, y.y, y.height) * 2 >= x.height.min(y.height);
    if same_line {
        // Spatial proximity alone does not make adjacent controls one sentence.
        return shared
            && a.table.is_none()
            && y.x >= x.x + x.width
            && y.x.saturating_sub(x.x + x.width) as f32 <= a.em * 0.7;
    }
    // A shared paragraph may be centered or right-aligned, and its first line
    // may be shorter than its continuation. Keep conservative alignment when
    // there is no common layout owner; separators still veto every join.
    let left_aligned =
        x.x.abs_diff(y.x) as f32 <= a.em * 0.7 && x.width as f32 >= y.width as f32 * 0.65;
    let centered = (2 * x.x + x.width).abs_diff(2 * y.x + y.width) as f32 <= a.em * 1.4;
    let right_aligned = (x.x + x.width).abs_diff(y.x + y.width) as f32 <= a.em * 0.7;
    if y.y <= x.y
        || !(left_aligned || (shared && (centered || right_aligned)))
        || y.y.saturating_sub(x.y + x.height) as f32 > a.em * (if shared { 1.6 } else { 0.8 })
        || overlap(x.x, x.width, y.x, y.width) * 4 < x.width.min(y.width) * 3
    {
        return false;
    }
    !separator(image, x, y, left.appearance.map(|s| s.background_rgb))
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.into_iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(y))
        .max()
        .unwrap_or(0)
}

fn same_ink_direction(a: [u8; 3], bg_a: [u8; 3], b: [u8; 3], bg_b: [u8; 3]) -> bool {
    // Antialiasing changes ink intensity, not its direction from the surface
    // color. Do not treat two samples of the same ink as different styles.
    let a = std::array::from_fn::<_, 3, _>(|i| f64::from(a[i]) - f64::from(bg_a[i]));
    let b = std::array::from_fn::<_, 3, _>(|i| f64::from(b[i]) - f64::from(bg_b[i]));
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let length_a: f64 = a.iter().map(|x| x * x).sum();
    let length_b: f64 = b.iter().map(|x| x * x).sum();
    dot > 0.0 && dot * dot >= 0.98 * length_a * length_b
}

fn contrast_boundary(a: [u8; 3], bg_a: [u8; 3], b: [u8; 3], bg_b: [u8; 3]) -> bool {
    // Modest intensity changes can be antialiasing. A large, corroborated
    // contrast change between lines preserves primary/secondary hierarchy.
    let a = u32::from(distance(a, bg_a));
    let b = u32::from(distance(b, bg_b));
    a.abs_diff(b) > 48 && a.max(b) * 5 > a.min(b) * 8
}

fn separator(
    image: &image::RgbaImage,
    a: PixelRegion,
    b: PixelRegion,
    background: Option<[u8; 3]>,
) -> bool {
    let Some(background) = background else {
        return false;
    };
    let left = a.x.min(b.x);
    let right = (a.x + a.width).max(b.x + b.width).min(image.width());
    let start = (a.y + a.height / 2).min(image.height());
    let end = (b.y + b.height / 2).min(image.height());
    (start..end).any(|y| {
        let count = (left..right)
            .filter(|&x| {
                let p = image.get_pixel(x, y).0;
                distance([p[0], p[1], p[2]], background) > 24
            })
            .count();
        count * 10 >= right.saturating_sub(left).max(1) as usize * 9
    })
}

fn encloses_other(members: &[usize], next: usize, features: &[Feature]) -> bool {
    let mut left = u32::MAX;
    let mut top = u32::MAX;
    let mut right = 0;
    let mut bottom = 0;
    for i in members.iter().copied().chain(std::iter::once(next)) {
        let b = features[i].box_;
        left = left.min(b.x);
        top = top.min(b.y);
        right = right.max(b.x + b.width);
        bottom = bottom.max(b.y + b.height);
    }
    features.iter().enumerate().any(|(i, f)| {
        i != next
            && !members.contains(&i)
            && overlap(left, right - left, f.box_.x, f.box_.width) * 2
                >= (right - left).min(f.box_.width)
            && overlap(top, bottom - top, f.box_.y, f.box_.height) * 2
                >= (bottom - top).min(f.box_.height)
    })
}
