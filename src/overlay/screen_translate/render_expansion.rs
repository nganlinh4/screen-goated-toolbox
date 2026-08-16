use super::geometry::PixelRegion;

pub(super) fn expand_vertical_surface(
    image: &image::RgbaImage,
    source: PixelRegion,
    members: &[PixelRegion],
    masks: &[PixelRegion],
    background: Option<([u8; 3], u8)>,
    translated_text: &str,
    preferred_font_size: f32,
) -> PixelRegion {
    if members.is_empty()
        || !members
            .iter()
            .all(|region| region.height > region.width.saturating_mul(3) / 2)
    {
        return source;
    }
    let Some((background, confidence)) = background else {
        return source;
    };
    if confidence < 45 || source.height == 0 || translated_text.trim().is_empty() {
        return source;
    }
    let glyph_area = preferred_font_size * preferred_font_size * 0.58;
    let desired_area = glyph_area * translated_text.chars().count().max(1) as f32;
    let desired_width = (desired_area / source.height as f32).ceil() as u32;
    let maximum_width = source
        .width
        .saturating_mul(3)
        .min(image.width().div_ceil(3))
        .max(source.width);
    let target_width = desired_width.clamp(source.width, maximum_width);
    if target_width <= source.width {
        return source;
    }

    let foreign_masks = masks
        .iter()
        .copied()
        .filter(|mask| !members.contains(mask))
        .collect::<Vec<_>>();
    let (left_limit, right_limit) = horizontal_corridor(source, image.width(), &foreign_masks);
    let mut expanded = source;
    let mut prefer_left = true;
    while expanded.width < target_width {
        let left = expanded.x.checked_sub(1).filter(|x| {
            *x >= left_limit
                && safe_column(
                    image,
                    *x,
                    expanded.y,
                    expanded.height,
                    background,
                    &foreign_masks,
                )
        });
        let right = expanded.x.checked_add(expanded.width).filter(|x| {
            *x < right_limit
                && *x < image.width()
                && safe_column(
                    image,
                    *x,
                    expanded.y,
                    expanded.height,
                    background,
                    &foreign_masks,
                )
        });
        let grow_left = match (left, right) {
            (Some(_), Some(_)) => prefer_left,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if grow_left {
            expanded.x -= 1;
        }
        expanded.width += 1;
        prefer_left = !prefer_left;
    }
    expanded
}

fn horizontal_corridor(
    source: PixelRegion,
    image_width: u32,
    foreign_masks: &[PixelRegion],
) -> (u32, u32) {
    let source_right = source.x.saturating_add(source.width);
    let mut left = 0;
    let mut right = image_width;
    for mask in foreign_masks
        .iter()
        .filter(|mask| ranges_overlap(source.y, source.height, mask.y, mask.height))
    {
        let mask_right = mask.x.saturating_add(mask.width);
        if mask_right <= source.x {
            left = left.max(mask_right.saturating_add(source.x).div_ceil(2));
        } else if mask.x >= source_right {
            right = right.min(source_right.saturating_add(mask.x).div_ceil(2));
        }
    }
    (left.min(source.x), right.max(source_right))
}

fn safe_column(
    image: &image::RgbaImage,
    x: u32,
    y: u32,
    height: u32,
    background: [u8; 3],
    foreign_masks: &[PixelRegion],
) -> bool {
    if foreign_masks.iter().any(|mask| {
        x >= mask.x && x < mask.x + mask.width && ranges_overlap(y, height, mask.y, mask.height)
    }) {
        return false;
    }
    let bottom = y.saturating_add(height).min(image.height());
    let mut similar = 0_u32;
    let mut longest_difference = 0_u32;
    let mut difference_run = 0_u32;
    for row in y..bottom {
        let pixel = image.get_pixel(x, row).0;
        if color_distance(pixel, background) <= 48 {
            similar += 1;
            difference_run = 0;
        } else {
            difference_run += 1;
            longest_difference = longest_difference.max(difference_run);
        }
    }
    let measured = bottom.saturating_sub(y).max(1);
    similar.saturating_mul(100) >= measured.saturating_mul(85)
        && longest_difference <= measured.div_ceil(12).max(2)
}

fn ranges_overlap(a: u32, a_size: u32, b: u32, b_size: u32) -> bool {
    a < b.saturating_add(b_size) && b < a.saturating_add(a_size)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_size_uses_glyph_height_for_rows_and_width_for_columns() {
        let image = image::RgbaImage::from_pixel(160, 160, image::Rgba([255, 255, 255, 255]));
        let row = PixelRegion {
            x: 0,
            y: 0,
            width: 120,
            height: 20,
        };
        let column = PixelRegion {
            x: 0,
            y: 0,
            width: 18,
            height: 140,
        };
        assert_eq!(preferred_font_size(&image, [(row, None)].into_iter()), 16.0);
        assert_eq!(
            preferred_font_size(&image, [(column, None)].into_iter()),
            14.0
        );
    }

    #[test]
    fn preferred_size_uses_ink_instead_of_ocr_box_padding() {
        let mut image = image::RgbaImage::from_pixel(120, 40, image::Rgba([255, 255, 255, 255]));
        for y in 10..24 {
            for x in 20..100 {
                image.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
            }
        }
        let region = PixelRegion {
            x: 10,
            y: 2,
            width: 100,
            height: 34,
        };
        assert_eq!(
            preferred_font_size(&image, [(region, Some(([255, 255, 255], 95)))].into_iter()),
            14.0
        );
    }

    #[test]
    fn preferred_size_does_not_count_a_detached_annotation_as_glyph_width() {
        let mut image = image::RgbaImage::from_pixel(80, 140, image::Rgba([255, 255, 255, 255]));
        for y in 10..130 {
            for x in 12..36 {
                image.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
            }
            for x in 50..56 {
                image.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
            }
        }
        let region = PixelRegion {
            x: 8,
            y: 4,
            width: 52,
            height: 132,
        };
        assert_eq!(
            preferred_font_size(&image, [(region, Some(([255, 255, 255], 95)))].into_iter()),
            24.0
        );
    }

    #[test]
    fn vertical_surface_expands_inside_background_edges_without_crossing_them() {
        let mut image = image::RgbaImage::from_pixel(100, 100, image::Rgba([250, 250, 250, 255]));
        for y in 10..90 {
            image.put_pixel(20, y, image::Rgba([0, 0, 0, 255]));
            image.put_pixel(80, y, image::Rgba([0, 0, 0, 255]));
        }
        let source = PixelRegion {
            x: 45,
            y: 20,
            width: 10,
            height: 60,
        };
        let expanded = expand_vertical_surface(
            &image,
            source,
            &[source],
            &[source],
            Some(([250, 250, 250], 95)),
            "a translated passage that needs horizontal room",
            14.0,
        );
        assert!(expanded.width > source.width);
        assert!(expanded.x > 20);
        assert!(expanded.x + expanded.width <= 80);
    }

    #[test]
    fn vertical_surface_never_claims_another_text_region() {
        let image = image::RgbaImage::from_pixel(100, 100, image::Rgba([250, 250, 250, 255]));
        let source = PixelRegion {
            x: 45,
            y: 20,
            width: 10,
            height: 60,
        };
        let neighbor = PixelRegion {
            x: 56,
            y: 20,
            width: 12,
            height: 60,
        };
        let expanded = expand_vertical_surface(
            &image,
            source,
            &[source],
            &[source, neighbor],
            Some(([250, 250, 250], 95)),
            "a translated passage that needs horizontal room",
            14.0,
        );
        assert!(expanded.x + expanded.width <= neighbor.x);
    }

    #[test]
    fn neighboring_vertical_surfaces_receive_nonoverlapping_corridors() {
        let image = image::RgbaImage::from_pixel(120, 100, image::Rgba([250, 250, 250, 255]));
        let left = PixelRegion {
            x: 20,
            y: 20,
            width: 10,
            height: 60,
        };
        let right = PixelRegion {
            x: 70,
            y: 20,
            width: 10,
            height: 60,
        };
        let masks = [left, right];
        let expand = |source| {
            expand_vertical_surface(
                &image,
                source,
                &[source],
                &masks,
                Some(([250, 250, 250], 95)),
                "a translated passage that needs horizontal room",
                14.0,
            )
        };
        let expanded_left = expand(left);
        let expanded_right = expand(right);
        assert!(expanded_left.x + expanded_left.width <= expanded_right.x);
    }
}
