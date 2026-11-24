//! Source-image geometry. Prompt coordinates are a derived serialization view.
use anyhow::{Result, bail};

use super::{contract::NormalizedBounds, geometry::PixelRegion};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceBounds {
    pixels: PixelRegion,
    image_width: u32,
    image_height: u32,
}

#[cfg(test)]
#[path = "source_bounds_tests.rs"]
mod tests;

impl SourceBounds {
    pub(crate) fn from_pixels(pixels: PixelRegion, width: u32, height: u32) -> Result<Self> {
        if width == 0
            || height == 0
            || pixels.width == 0
            || pixels.height == 0
            || u64::from(pixels.x) + u64::from(pixels.width) > u64::from(width)
            || u64::from(pixels.y) + u64::from(pixels.height) > u64::from(height)
        {
            bail!("source geometry is outside its image");
        }
        Ok(Self {
            pixels,
            image_width: width,
            image_height: height,
        })
    }

    pub(crate) fn from_quad(quad: &[[f32; 2]; 4], width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || quad.iter().flatten().any(|value| !value.is_finite()) {
            bail!("source geometry has invalid coordinates or image dimensions");
        }
        let left = quad
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let top = quad
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let right = quad
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = quad
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        Self::enclosing(
            f64::from(left),
            f64::from(top),
            f64::from(right),
            f64::from(bottom),
            width,
            height,
        )
    }

    fn enclosing(
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let left = left.floor().clamp(0.0, f64::from(width)) as u32;
        let top = top.floor().clamp(0.0, f64::from(height)) as u32;
        let right = right.ceil().clamp(0.0, f64::from(width)) as u32;
        let bottom = bottom.ceil().clamp(0.0, f64::from(height)) as u32;
        Self::from_pixels(
            PixelRegion {
                x: left,
                y: top,
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            },
            width,
            height,
        )
    }

    /// Import a coordinate format whose only available geometry is normalized.
    #[cfg(any(debug_assertions, test))]
    pub(crate) fn from_normalized(
        bounds: NormalizedBounds,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        if bounds.left >= bounds.right
            || bounds.top >= bounds.bottom
            || bounds.right > 1000
            || bounds.bottom > 1000
        {
            bail!("normalized source geometry is invalid");
        }
        Self::from_pixels(
            super::geometry::normalized_region(bounds, width, height),
            width,
            height,
        )
    }

    pub(crate) fn pixels(self) -> PixelRegion {
        self.pixels
    }

    pub(crate) fn image_size(self) -> (u32, u32) {
        (self.image_width, self.image_height)
    }

    pub(crate) fn prompt(self) -> NormalizedBounds {
        let coordinate = |value: u32, extent: u32| {
            ((u64::from(value) * 1000 + u64::from(extent) / 2) / u64::from(extent)).min(1000) as u16
        };
        let left = coordinate(self.pixels.x, self.image_width).min(999);
        let top = coordinate(self.pixels.y, self.image_height).min(999);
        NormalizedBounds {
            left,
            top,
            right: coordinate(self.pixels.x + self.pixels.width, self.image_width).max(left + 1),
            bottom: coordinate(self.pixels.y + self.pixels.height, self.image_height).max(top + 1),
        }
    }

    #[cfg(any(debug_assertions, test))]
    pub(crate) fn rescale(self, width: u32, height: u32) -> Result<Self> {
        if self.image_size() == (width, height) {
            return Ok(self);
        }
        let x = f64::from(width) / f64::from(self.image_width);
        let y = f64::from(height) / f64::from(self.image_height);
        Self::enclosing(
            f64::from(self.pixels.x) * x,
            f64::from(self.pixels.y) * y,
            f64::from(self.pixels.x + self.pixels.width) * x,
            f64::from(self.pixels.y + self.pixels.height) * y,
            width,
            height,
        )
    }

    pub(crate) fn union(bounds: impl IntoIterator<Item = Self>) -> Result<Self> {
        let mut bounds = bounds.into_iter();
        let Some(mut result) = bounds.next() else {
            bail!("source geometry union is empty");
        };
        for item in bounds {
            if item.image_size() != result.image_size() {
                bail!("source geometry belongs to different images");
            }
            let right =
                (result.pixels.x + result.pixels.width).max(item.pixels.x + item.pixels.width);
            let bottom =
                (result.pixels.y + result.pixels.height).max(item.pixels.y + item.pixels.height);
            result.pixels.x = result.pixels.x.min(item.pixels.x);
            result.pixels.y = result.pixels.y.min(item.pixels.y);
            result.pixels.width = right - result.pixels.x;
            result.pixels.height = bottom - result.pixels.y;
        }
        Ok(result)
    }
}
