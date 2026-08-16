use image::RgbImage;
use sgt_screen_text_detector_protocol::DetectedRegion;

pub(crate) fn split(image: &RgbImage, regions: Vec<DetectedRegion>) -> Vec<DetectedRegion> {
    regions
        .into_iter()
        .flat_map(|region| split_row(image, region))
        .collect()
}

fn split_row(image: &RgbImage, region: DetectedRegion) -> Vec<DetectedRegion> {
    let width = region.right.saturating_sub(region.left);
    let height = region.bottom.saturating_sub(region.top);
    if height == 0 || width < height.saturating_mul(5) {
        return vec![region];
    }
    let background = border_median(image, &region);
    let required_ink = height.div_ceil(12).max(1);
    let occupied = (region.left..region.right)
        .map(|x| {
            (region.top..region.bottom)
                .filter(|&y| color_distance(*image.get_pixel(x, y), background) >= 36)
                .take(required_ink as usize)
                .count() as u32
                >= required_ink
        })
        .collect::<Vec<_>>();
    let Some(first) = occupied.iter().position(|value| *value) else {
        return vec![region];
    };
    let last = occupied.iter().rposition(|value| *value).unwrap_or(first);
    let minimum_gap = usize::try_from(height.div_ceil(2).max(6)).unwrap_or(usize::MAX);
    let minimum_segment = usize::try_from(height.max(12)).unwrap_or(usize::MAX);
    let mut cuts = Vec::new();
    let mut gap_start = None;
    for (index, ink) in occupied.iter().enumerate().take(last + 1).skip(first) {
        if !ink {
            gap_start.get_or_insert(index);
        } else if let Some(start) = gap_start.take() {
            record_cut(
                &mut cuts,
                start,
                index,
                first,
                last,
                minimum_gap,
                minimum_segment,
            );
        }
    }
    if let Some(start) = gap_start {
        record_cut(
            &mut cuts,
            start,
            last + 1,
            first,
            last,
            minimum_gap,
            minimum_segment,
        );
    }
    if cuts.is_empty() {
        return vec![region];
    }
    let mut result = Vec::with_capacity(cuts.len() + 1);
    let mut start = first;
    for end in cuts.into_iter().chain(std::iter::once(last + 1)) {
        let mut split = region.clone();
        split.left = region
            .left
            .saturating_add(u32::try_from(start).unwrap_or(0));
        split.right = region
            .left
            .saturating_add(u32::try_from(end.min(occupied.len())).unwrap_or(width));
        if split.right.saturating_sub(split.left) >= 4 {
            result.push(split);
        }
        start = end;
    }
    if result.len() > 1 {
        result
    } else {
        vec![region]
    }
}

fn record_cut(
    cuts: &mut Vec<usize>,
    start: usize,
    end: usize,
    first: usize,
    last: usize,
    minimum_gap: usize,
    minimum_segment: usize,
) {
    if end.saturating_sub(start) >= minimum_gap
        && start.saturating_sub(first) >= minimum_segment
        && last.saturating_add(1).saturating_sub(end) >= minimum_segment
    {
        cuts.push((start + end) / 2);
    }
}

fn border_median(image: &RgbImage, region: &DetectedRegion) -> image::Rgb<u8> {
    let mut channels = [Vec::new(), Vec::new(), Vec::new()];
    for x in region.left..region.right {
        for y in [region.top, region.bottom.saturating_sub(1)] {
            for (values, channel) in channels.iter_mut().zip(image.get_pixel(x, y).0) {
                values.push(channel);
            }
        }
    }
    for y in region.top..region.bottom {
        for x in [region.left, region.right.saturating_sub(1)] {
            for (values, channel) in channels.iter_mut().zip(image.get_pixel(x, y).0) {
                values.push(channel);
            }
        }
    }
    image::Rgb(channels.map(|mut values| {
        values.sort_unstable();
        values.get(values.len() / 2).copied().unwrap_or(0)
    }))
}

fn color_distance(left: image::Rgb<u8>, right: image::Rgb<u8>) -> u8 {
    left[0]
        .abs_diff(right[0])
        .max(left[1].abs_diff(right[1]))
        .max(left[2].abs_diff(right[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_locator_row_is_split_only_at_a_large_visual_gap() {
        let mut image = RgbImage::from_pixel(180, 32, image::Rgb([12, 12, 12]));
        for x in 12..70 {
            for y in 8..24 {
                image.put_pixel(x, y, image::Rgb([240, 240, 240]));
            }
        }
        for x in 105..166 {
            for y in 8..24 {
                image.put_pixel(x, y, image::Rgb([240, 240, 240]));
            }
        }
        let region = DetectedRegion {
            left: 4,
            top: 4,
            right: 176,
            bottom: 28,
            confidence: 0.9,
            text: String::new(),
            text_confidence: 0.0,
            alternatives: Vec::new(),
        };
        let split = split_row(&image, region);
        assert_eq!(split.len(), 2);
        assert!(split[0].right <= split[1].left);
    }
}
