use eframe::egui;

pub(super) const MAIN_WIDTH: f32 = 240.0;
pub(super) const MAIN_HEIGHT: f32 = 186.0;
pub(super) const FLYOUT_WIDTH: f32 = 236.0;
pub(super) const FLYOUT_GAP: f32 = 10.0;
pub(super) const OPTION_HEIGHT: f32 = 28.0;
const FLYOUT_VERTICAL_PADDING: f32 = 8.0;
const FLYOUT_TOP_INSET: f32 = 6.0;
const FLYOUT_PREFERRED_TOP: f32 = 100.0;
const CURSOR_GAP: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PopupPlacement {
    pub size_points: egui::Vec2,
    pub physical_position: PhysicalPoint,
    pub physical_size: [i32; 2],
    pub flyout_top: f32,
    pub flyout_height: f32,
}

impl PopupPlacement {
    pub fn has_flyout(self) -> bool {
        self.flyout_height > 0.0
    }
}

pub(super) fn place(
    anchor: PhysicalPoint,
    work_area: WorkArea,
    pixels_per_point: f32,
    option_count: usize,
) -> PopupPlacement {
    let pixels_per_point = pixels_per_point.max(0.5);
    let flyout_height = flyout_height(option_count);
    let logical_width = MAIN_WIDTH
        + if option_count == 0 {
            0.0
        } else {
            FLYOUT_GAP + FLYOUT_WIDTH
        };
    let logical_size = egui::vec2(logical_width, MAIN_HEIGHT);
    let physical_width = (logical_width * pixels_per_point).round() as i32;
    let physical_height = (MAIN_HEIGHT * pixels_per_point).round() as i32;
    let main_width = (MAIN_WIDTH * pixels_per_point).round() as i32;
    let cursor_gap = (CURSOR_GAP * pixels_per_point).round() as i32;

    let unclamped_x = anchor.x - main_width / 2;
    let unclamped_y = anchor.y - physical_height - cursor_gap;
    let x = clamp_axis(unclamped_x, work_area.left, work_area.right, physical_width);
    let y = clamp_axis(
        unclamped_y,
        work_area.top,
        work_area.bottom,
        physical_height,
    );

    PopupPlacement {
        size_points: logical_size,
        physical_position: PhysicalPoint { x, y },
        physical_size: [physical_width, physical_height],
        flyout_top: flyout_top(option_count),
        flyout_height,
    }
}

fn clamp_axis(value: i32, start: i32, end: i32, extent: i32) -> i32 {
    value.max(start).min((end - extent).max(start))
}

fn flyout_height(option_count: usize) -> f32 {
    if option_count == 0 {
        0.0
    } else {
        FLYOUT_VERTICAL_PADDING + option_count as f32 * OPTION_HEIGHT
    }
}

fn flyout_top(option_count: usize) -> f32 {
    let height = flyout_height(option_count);
    if height == 0.0 {
        return FLYOUT_TOP_INSET;
    }
    let max_top = (MAIN_HEIGHT - height - FLYOUT_TOP_INSET).max(FLYOUT_TOP_INSET);
    FLYOUT_PREFERRED_TOP.clamp(FLYOUT_TOP_INSET, max_top)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY_WORK: WorkArea = WorkArea {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    #[test]
    fn main_card_is_centered_above_the_tray_anchor() {
        let placement = place(PhysicalPoint { x: 960, y: 1030 }, PRIMARY_WORK, 1.0, 0);
        assert_eq!(
            placement.physical_position,
            PhysicalPoint { x: 840, y: 834 }
        );
        assert_eq!(placement.physical_size, [240, 186]);
    }

    #[test]
    fn flyout_reserves_space_and_stays_inside_the_monitor() {
        let placement = place(PhysicalPoint { x: 1900, y: 1030 }, PRIMARY_WORK, 1.0, 5);
        assert_eq!(placement.physical_position.x, 1920 - 486);
        assert_eq!(placement.physical_size, [486, 186]);
        assert_eq!(placement.flyout_top, 32.0);
        assert_eq!(placement.flyout_height, 148.0);
    }

    #[test]
    fn negative_monitor_origins_are_preserved() {
        let work = WorkArea {
            left: -1920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        let placement = place(PhysicalPoint { x: -1800, y: 870 }, work, 1.5, 0);
        assert_eq!(placement.physical_position.x, -1920);
        assert!(placement.physical_position.y >= -200);
        assert_eq!(placement.physical_size, [360, 279]);
    }

    #[test]
    fn logical_geometry_is_stable_across_dpi() {
        let placement = place(PhysicalPoint { x: 1200, y: 900 }, PRIMARY_WORK, 2.0, 3);
        assert_eq!(placement.size_points, egui::vec2(486.0, 186.0));
        assert_eq!(placement.physical_size, [972, 372]);
    }
}
