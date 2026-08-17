use super::contract::NormalizedBounds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PixelRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub(super) const MIN_READABLE_WIDTH: u32 = 4;
pub(super) const MIN_READABLE_HEIGHT: u32 = 8;

pub(crate) fn normalized_region(
    bounds: NormalizedBounds,
    image_width: u32,
    image_height: u32,
) -> PixelRegion {
    let scale =
        |value: u16, extent: u32| ((u64::from(value) * u64::from(extent) + 500) / 1000) as u32;
    let x = scale(bounds.left, image_width).min(image_width.saturating_sub(1));
    let y = scale(bounds.top, image_height).min(image_height.saturating_sub(1));
    let right = scale(bounds.right, image_width).clamp(x + 1, image_width.max(1));
    let bottom = scale(bounds.bottom, image_height).clamp(y + 1, image_height.max(1));
    PixelRegion {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

pub(crate) fn background_sample_region(
    region: PixelRegion,
    image_width: u32,
    image_height: u32,
) -> PixelRegion {
    let margin = region.height.div_ceil(2).clamp(4, 24);
    let x = region.x.saturating_sub(margin);
    let y = region.y.saturating_sub(margin);
    let right = region
        .x
        .saturating_add(region.width)
        .saturating_add(margin)
        .min(image_width);
    let bottom = region
        .y
        .saturating_add(region.height)
        .saturating_add(margin)
        .min(image_height);
    PixelRegion {
        x,
        y,
        width: right.saturating_sub(x).max(1),
        height: bottom.saturating_sub(y).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_geometry_scales_width_and_height_independently() {
        let region = normalized_region(
            NormalizedBounds {
                left: 100,
                top: 200,
                right: 600,
                bottom: 700,
            },
            1200,
            300,
        );
        assert_eq!(
            region,
            PixelRegion {
                x: 120,
                y: 60,
                width: 600,
                height: 150
            }
        );
    }

    #[test]
    fn physical_readability_gate_uses_region_height() {
        let artifact = normalized_region(
            NormalizedBounds {
                left: 100,
                top: 200,
                right: 110,
                bottom: 205,
            },
            1200,
            600,
        );
        assert!(artifact.width >= MIN_READABLE_WIDTH);
        assert!(artifact.height < MIN_READABLE_HEIGHT);
    }

    #[test]
    fn background_sampling_surrounds_the_detector_region() {
        let region = PixelRegion {
            x: 20,
            y: 12,
            width: 20,
            height: 10,
        };
        assert_eq!(
            background_sample_region(region, 80, 40),
            PixelRegion {
                x: 15,
                y: 7,
                width: 30,
                height: 20,
            }
        );
    }

    #[test]
    fn background_sampling_is_clipped_at_capture_edges() {
        let region = PixelRegion {
            x: 0,
            y: 2,
            width: 20,
            height: 10,
        };
        assert_eq!(
            background_sample_region(region, 24, 14),
            PixelRegion {
                x: 0,
                y: 0,
                width: 24,
                height: 14,
            }
        );
    }
}
