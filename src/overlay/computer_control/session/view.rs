//! Screen-space image regions and their shared coordinate transforms.

use anyhow::Result;

use super::Capture;

/// Exact screen-pixel rectangle represented by a model image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct View {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl View {
    pub fn intersection(self, bounds: Self) -> Option<Self> {
        if self.w <= 0 || self.h <= 0 || bounds.w <= 0 || bounds.h <= 0 {
            return None;
        }
        let left = i64::from(self.x.max(bounds.x));
        let top = i64::from(self.y.max(bounds.y));
        let right =
            (i64::from(self.x) + i64::from(self.w)).min(i64::from(bounds.x) + i64::from(bounds.w));
        let bottom =
            (i64::from(self.y) + i64::from(self.h)).min(i64::from(bounds.y) + i64::from(bounds.h));
        (right > left && bottom > top).then(|| Self {
            x: left as i32,
            y: top as i32,
            w: (right - left) as i32,
            h: (bottom - top) as i32,
        })
    }

    /// Select the whole image for an empty or sub-eight-pixel intersection.
    pub fn clamped_to(self, bounds: Self) -> Self {
        self.intersection(bounds)
            .filter(|view| view.w >= 8 && view.h >= 8)
            .unwrap_or(bounds)
    }

    pub fn to_screen_px(self, mx: f64, my: f64) -> (i32, i32) {
        let max_x = f64::from(self.w.saturating_sub(1).max(0));
        let max_y = f64::from(self.h.saturating_sub(1).max(0));
        (
            self.x
                .saturating_add((mx.clamp(0.0, 1000.0) / 1000.0 * max_x).round() as i32),
            self.y
                .saturating_add((my.clamp(0.0, 1000.0) / 1000.0 * max_y).round() as i32),
        )
    }

    pub fn screen_to_norm(self, x: i32, y: i32) -> (f64, f64) {
        (
            ((f64::from(x) - f64::from(self.x)) / f64::from(self.w.saturating_sub(1).max(1))
                * 1000.0)
                .clamp(0.0, 1000.0),
            ((f64::from(y) - f64::from(self.y)) / f64::from(self.h.saturating_sub(1).max(1))
                * 1000.0)
                .clamp(0.0, 1000.0),
        )
    }
}

impl Capture {
    pub fn bounds(&self) -> Result<View> {
        let w = i32::try_from(self.rgb.width())?;
        let h = i32::try_from(self.rgb.height())?;
        anyhow::ensure!(w > 0 && h > 0, "screen capture is empty");
        anyhow::ensure!(
            self.origin_x.checked_add(w - 1).is_some()
                && self.origin_y.checked_add(h - 1).is_some(),
            "screen capture coordinates exceed their range"
        );
        Ok(View {
            x: self.origin_x,
            y: self.origin_y,
            w,
            h,
        })
    }
}

#[cfg(test)]
mod tests;
