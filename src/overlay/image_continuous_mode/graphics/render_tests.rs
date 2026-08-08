use super::*;

const SIZE: (i32, i32) = (96, 72);
const DIM_ALPHA: u8 = 100;

fn source_pixels() -> Vec<u32> {
    (0..SIZE.0 * SIZE.1)
        .map(|index| {
            let red = (index * 17 % 256) as u32;
            let green = (index * 31 % 256) as u32;
            let blue = (index * 47 % 256) as u32;
            (red << 16) | (green << 8) | blue
        })
        .collect()
}

fn render_full(source: &[u32], selection: RECT) -> Vec<u32> {
    let mut rendered = source.to_vec();
    render_frozen_with_selection(FrozenSelectionRender {
        pixels: &mut rendered,
        size: SIZE,
        selection,
        dim_alpha: DIM_ALPHA,
    });
    rendered
}

fn restore_region(target: &mut [u32], source: &[u32], region: RECT) {
    for y in region.top..region.bottom {
        let start = (y * SIZE.0 + region.left) as usize;
        let end = (y * SIZE.0 + region.right) as usize;
        target[start..end].copy_from_slice(&source[start..end]);
    }
}

fn assert_dirty_transition_matches_full_render(previous: RECT, current: RECT) {
    let source = source_pixels();
    let mut incremental = render_full(&source, previous);
    let damage = damaged_region(Some(previous), Some(current), SIZE).unwrap();
    restore_region(&mut incremental, &source, damage);
    render_frozen_clip(&mut incremental, SIZE, Some(current), DIM_ALPHA, damage);

    let expected = render_full(&source, current);
    if let Some((index, (actual, expected))) = incremental
        .iter()
        .zip(&expected)
        .enumerate()
        .find(|(_, (actual, expected))| actual != expected)
    {
        panic!(
            "pixel mismatch at ({}, {}): actual={actual:#010x} expected={expected:#010x}",
            index as i32 % SIZE.0,
            index as i32 / SIZE.0
        );
    }
}

#[test]
fn dirty_render_matches_full_render_when_selection_grows() {
    assert_dirty_transition_matches_full_render(
        RECT {
            left: 20,
            top: 18,
            right: 45,
            bottom: 38,
        },
        RECT {
            left: 20,
            top: 18,
            right: 82,
            bottom: 60,
        },
    );
}

#[test]
fn dirty_render_matches_full_render_when_selection_moves_and_shrinks() {
    assert_dirty_transition_matches_full_render(
        RECT {
            left: 8,
            top: 7,
            right: 88,
            bottom: 65,
        },
        RECT {
            left: 35,
            top: 24,
            right: 62,
            bottom: 48,
        },
    );
}

#[test]
fn first_drag_frame_has_visible_dim_alpha() {
    assert_eq!(stepped_dim_alpha(0, TARGET_DIM_ALPHA), DIM_FADE_STEP);
    assert!(stepped_dim_alpha(0, TARGET_DIM_ALPHA) > 0);
}
