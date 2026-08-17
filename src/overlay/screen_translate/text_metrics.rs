use super::geometry::PixelRegion;

pub(super) fn preferred_font_size(
    image: &image::RgbaImage,
    regions: impl Iterator<Item = (PixelRegion, Option<([u8; 3], u8)>)>,
) -> f32 {
    let mut em_sizes = regions
        .map(|(region, background)| {
            ink_em_size(image, region, background).unwrap_or_else(|| {
                let box_em = if region.height > region.width.saturating_mul(3) / 2 {
                    region.width
                } else {
                    region.height
                };
                (box_em as f32 * 0.78).round().max(1.0) as u32
            })
        })
        .collect::<Vec<_>>();
    em_sizes.sort_unstable();
    let em = em_sizes.get(em_sizes.len() / 2).copied().unwrap_or(8);
    (em as f32).clamp(7.0, 200.0)
}

fn ink_em_size(
    image: &image::RgbaImage,
    region: PixelRegion,
    background: Option<([u8; 3], u8)>,
) -> Option<u32> {
    let (background, confidence) = background?;
    if confidence < 45 || region.width == 0 || region.height == 0 {
        return None;
    }
    let right = region.x.saturating_add(region.width).min(image.width());
    let bottom = region.y.saturating_add(region.height).min(image.height());
    let vertical = region.height > region.width.saturating_mul(3) / 2;
    let threshold = if confidence >= 75 { 38 } else { 52 };
    let (major_start, major_end, minor_start, minor_end) = if vertical {
        (region.x, right, region.y, bottom)
    } else {
        (region.y, bottom, region.x, right)
    };
    let required = (minor_end.saturating_sub(minor_start) / 80).max(1);
    let occupied = (major_start..major_end)
        .map(|major| {
            let ink = (minor_start..minor_end)
                .filter(|&minor| {
                    let pixel = if vertical {
                        image.get_pixel(major, minor).0
                    } else {
                        image.get_pixel(minor, major).0
                    };
                    color_distance(pixel, background) >= threshold
                })
                .count() as u32;
            ink >= required
        })
        .collect::<Vec<_>>();
    longest_ink_cluster(&occupied)
}

fn longest_ink_cluster(occupied: &[bool]) -> Option<u32> {
    let gap_tolerance = (occupied.len() / 24).clamp(1, 3);
    let mut best = 0usize;
    let mut start = None;
    let mut last_ink = None;
    for (index, ink) in occupied.iter().copied().enumerate() {
        if ink {
            start.get_or_insert(index);
            last_ink = Some(index);
        } else if let (Some(cluster_start), Some(last)) = (start, last_ink)
            && index.saturating_sub(last) > gap_tolerance
        {
            best = best.max(last.saturating_sub(cluster_start) + 1);
            start = None;
            last_ink = None;
        }
    }
    if let (Some(cluster_start), Some(last)) = (start, last_ink) {
        best = best.max(last.saturating_sub(cluster_start) + 1);
    }
    (best > 0).then(|| u32::try_from(best).unwrap_or(u32::MAX))
}

fn color_distance(pixel: [u8; 4], background: [u8; 3]) -> u8 {
    pixel[0]
        .abs_diff(background[0])
        .max(pixel[1].abs_diff(background[1]))
        .max(pixel[2].abs_diff(background[2]))
}
