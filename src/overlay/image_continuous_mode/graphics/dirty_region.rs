use windows::Win32::Foundation::RECT;

const BORDER_DAMAGE_PADDING: i32 = 12;

pub(super) fn selection_rect(
    start: (i32, i32),
    end: (i32, i32),
    virtual_origin: (i32, i32),
) -> Option<RECT> {
    let rect = RECT {
        left: start.0.min(end.0) - virtual_origin.0,
        top: start.1.min(end.1) - virtual_origin.1,
        right: start.0.max(end.0) - virtual_origin.0,
        bottom: start.1.max(end.1) - virtual_origin.1,
    };
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

pub(super) fn damaged_region(
    previous: Option<RECT>,
    current: Option<RECT>,
    size: (i32, i32),
) -> Option<RECT> {
    let mut damage = match (previous, current) {
        (Some(previous), Some(current)) => RECT {
            left: previous.left.min(current.left),
            top: previous.top.min(current.top),
            right: previous.right.max(current.right),
            bottom: previous.bottom.max(current.bottom),
        },
        (Some(rect), None) | (None, Some(rect)) => rect,
        (None, None) => return None,
    };
    damage.left = (damage.left - BORDER_DAMAGE_PADDING).clamp(0, size.0);
    damage.top = (damage.top - BORDER_DAMAGE_PADDING).clamp(0, size.1);
    damage.right = (damage.right + BORDER_DAMAGE_PADDING).clamp(0, size.0);
    damage.bottom = (damage.bottom + BORDER_DAMAGE_PADDING).clamp(0, size.1);
    (damage.left < damage.right && damage.top < damage.bottom).then_some(damage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_coordinates_are_local_to_the_virtual_desktop() {
        assert_eq!(
            selection_rect((-1800, 200), (-1200, 700), (-1920, 0)),
            Some(RECT {
                left: 120,
                top: 200,
                right: 720,
                bottom: 700,
            })
        );
    }

    #[test]
    fn crossing_the_drag_origin_keeps_an_ordered_rectangle() {
        assert_eq!(
            selection_rect((500, 500), (200, 100), (0, 0)),
            Some(RECT {
                left: 200,
                top: 100,
                right: 500,
                bottom: 500,
            })
        );
    }

    #[test]
    fn damage_restores_both_old_and_new_selection_bounds() {
        let old = RECT {
            left: 100,
            top: 100,
            right: 500,
            bottom: 400,
        };
        let new = RECT {
            left: 300,
            top: 80,
            right: 700,
            bottom: 250,
        };
        assert_eq!(
            damaged_region(Some(old), Some(new), (800, 600)),
            Some(RECT {
                left: 88,
                top: 68,
                right: 712,
                bottom: 412,
            })
        );
    }

    #[test]
    fn damage_is_clipped_to_the_virtual_desktop() {
        let edge = RECT {
            left: 0,
            top: 0,
            right: 800,
            bottom: 600,
        };
        assert_eq!(damaged_region(Some(edge), None, (800, 600)), Some(edge));
    }
}
