//! The splash's single pixel grid.
//!
//! Every splash element (the scene framebuffer, the pixel-font text, the
//! progress bar and the voxels that escape the window) lands on the same square
//! cells. A cell is a whole number of physical pixels, so nearest-neighbour
//! upscaling gives every scene pixel the same size. A fractional scale would mix
//! 3- and 4-pixel cells, and anything drawn over it would drift off the scene.

use eframe::egui::Vec2;

/// Rows the framebuffer aims for; this sets how chunky the art is. The real
/// row count follows from the whole-pixel cell size, so it moves slightly with
/// the display scale (240 rows at 100%, 225 at 125%, 216 at 150%).
const TARGET_ROWS: f32 = 230.0;
/// Upper bound on either framebuffer dimension, keeping the CPU raster cheap.
const MAX_CELLS: f32 = 1024.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PixelGrid {
    /// Edge of one cell in physical pixels.
    pub cell_px: u32,
    /// Edge of one cell in logical points.
    pub cell: f32,
    pub cols: usize,
    pub rows: usize,
}

impl PixelGrid {
    /// The grid for a window of `size` logical points, anchored at its top-left.
    pub(crate) fn new(size: Vec2, pixels_per_point: f32) -> Self {
        let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            pixels_per_point
        } else {
            1.0
        };
        let cell_px = (size.y * ppp / TARGET_ROWS).round().max(1.0) as u32;
        let cell = cell_px as f32 / ppp;
        let cells = |extent: f32| (extent / cell).ceil().clamp(16.0, MAX_CELLS) as usize;
        Self {
            cell_px,
            cell,
            cols: cells(size.x),
            rows: cells(size.y),
        }
    }

    /// Logical points to framebuffer cells.
    pub(crate) fn scale(&self) -> f32 {
        1.0 / self.cell
    }

    /// Snaps a logical coordinate to the nearest cell boundary.
    pub(crate) fn snap(&self, v: f32) -> f32 {
        (v / self.cell).round() * self.cell
    }

    /// Logical area the framebuffer covers. It overhangs the window by less
    /// than one cell, so cells never stretch to fill a remainder.
    pub(crate) fn extent(&self) -> Vec2 {
        Vec2::new(self.cols as f32, self.rows as f32) * self.cell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPLASH: Vec2 = Vec2::new(1000.0, 720.0);

    #[test]
    fn cells_are_whole_physical_pixels_at_common_scales() {
        for (ppp, cell_px) in [(1.0, 3), (1.25, 4), (1.5, 5), (2.0, 6)] {
            let grid = PixelGrid::new(SPLASH, ppp);
            assert_eq!(grid.cell_px, cell_px, "scale {ppp}");
            assert!((grid.cell * ppp - cell_px as f32).abs() < 1e-4);
        }
    }

    #[test]
    fn framebuffer_covers_the_window_without_stretching_a_cell() {
        for ppp in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let grid = PixelGrid::new(SPLASH, ppp);
            let extent = grid.extent();
            assert!(extent.x >= SPLASH.x && extent.x - SPLASH.x < grid.cell);
            assert!(extent.y >= SPLASH.y && extent.y - SPLASH.y < grid.cell);
            // Every cell boundary is an exact physical pixel.
            let right_edge_px = extent.x * ppp;
            assert!((right_edge_px - right_edge_px.round()).abs() < 1e-3);
        }
    }

    #[test]
    fn snapped_coordinates_sit_on_cell_boundaries() {
        let grid = PixelGrid::new(SPLASH, 1.25);
        for v in [0.0, 13.7, 418.5, 538.0, 999.9] {
            let cells = grid.snap(v) / grid.cell;
            assert!((cells - cells.round()).abs() < 1e-4, "{v}");
            assert!((grid.snap(v) - v).abs() <= grid.cell * 0.5 + 1e-4);
        }
    }

    #[test]
    fn degenerate_scale_falls_back_to_one_point_per_pixel() {
        assert_eq!(
            PixelGrid::new(SPLASH, 0.0),
            PixelGrid::new(SPLASH, f32::NAN)
        );
        assert_eq!(PixelGrid::new(SPLASH, 0.0).cell_px, 3);
    }
}
