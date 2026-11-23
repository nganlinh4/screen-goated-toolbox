//! Anti-aliased rounded selection outline shared by the selection overlays.

/// Width of the selection outline, in physical pixels.
pub(crate) const BORDER_WIDTH: f32 = 2.0;

/// A selection rectangle with the shared corner radius, shrunk for small boxes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RoundedSelection {
    center_x: f32,
    center_y: f32,
    half_width: f32,
    half_height: f32,
    radius: f32,
}

impl RoundedSelection {
    pub(crate) fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        let half_width = (right - left) as f32 / 2.0;
        let half_height = (bottom - top) as f32 / 2.0;
        Self {
            center_x: left as f32 + half_width,
            center_y: top as f32 + half_height,
            half_width,
            half_height,
            radius: crate::overlay::BOX_CORNER_RADIUS_PHYSICAL_PX
                .min(half_width)
                .min(half_height),
        }
    }

    /// Whole pixels covered by each corner, i.e. the rows that need per-pixel shading.
    pub(crate) fn corner_rows(&self) -> i32 {
        self.radius.ceil() as i32
    }

    /// Coverage of pixel (`x`, `y`): `(inside, border)`, each in 0..=1, where
    /// `inside` includes the border and `border` is the outline alone.
    pub(crate) fn coverage(&self, x: i32, y: i32) -> (f32, f32) {
        let dx = (x as f32 + 0.5 - self.center_x).abs() - (self.half_width - self.radius);
        let dy = (y as f32 + 0.5 - self.center_y).abs() - (self.half_height - self.radius);
        let distance = if dx > 0.0 && dy > 0.0 {
            (dx * dx + dy * dy).sqrt() - self.radius
        } else {
            dx.max(dy) - self.radius
        };
        let inside = (0.5 - distance).clamp(0.0, 1.0);
        let interior = (0.5 - (distance + BORDER_WIDTH)).clamp(0.0, 1.0);
        (inside, inside - interior)
    }
}

#[cfg(test)]
mod tests {
    use super::RoundedSelection;

    #[test]
    fn coverage_separates_outline_interior_and_rounded_corner() {
        let selection = RoundedSelection::new(10, 10, 110, 60);
        assert_eq!(selection.coverage(60, 35), (1.0, 0.0), "interior");
        assert_eq!(selection.coverage(10, 35), (1.0, 1.0), "left outline");
        assert_eq!(selection.coverage(5, 35), (0.0, 0.0), "outside");
        // The exact corner pixel lies outside the rounded outline.
        assert_eq!(selection.coverage(10, 10).0, 0.0);
        assert_eq!(selection.corner_rows(), 8);
        assert_eq!(RoundedSelection::new(0, 0, 6, 6).corner_rows(), 3);
    }
}
