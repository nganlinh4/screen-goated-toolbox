use super::{dim_pixels, fix_alpha};
use windows::Win32::Foundation::RECT;

struct CornerGeometry {
    border_x: (i32, i32),
    center: (f32, f32),
    half_size: (f32, f32),
    radius: f32,
    dim_f: f32,
    inv_dim: u32,
}

pub(super) fn render_frozen_clip(
    pixels: &mut [u32],
    size: (i32, i32),
    selection: Option<RECT>,
    dim_alpha: u8,
    clip: RECT,
) {
    let (width, height) = size;
    let clip = RECT {
        left: clip.left.clamp(0, width),
        top: clip.top.clamp(0, height),
        right: clip.right.clamp(0, width),
        bottom: clip.bottom.clamp(0, height),
    };
    if clip.left >= clip.right || clip.top >= clip.bottom {
        return;
    }
    let Some(selection) = selection else {
        dim_rect(pixels, width, clip, 256 - dim_alpha as u32);
        return;
    };

    let inv_dim = 256u32 - dim_alpha as u32;
    let dim_f = dim_alpha as f32 / 255.0;
    let half_width = (selection.right - selection.left) as f32 / 2.0;
    let half_height = (selection.bottom - selection.top) as f32 / 2.0;
    let center_x = selection.left as f32 + half_width;
    let center_y = selection.top as f32 + half_height;
    let radius = 8.0f32.min(half_width).min(half_height);
    let border_left = (selection.left - 10).max(0);
    let border_right = (selection.right + 10).min(width);
    let border_top = (selection.top - 10).max(0);
    let border_bottom = (selection.bottom + 10).min(height);
    let radius_pixels = radius.ceil() as i32;
    let top_band_end = (selection.top + radius_pixels).min(border_bottom);
    let bottom_band_start = (selection.bottom - radius_pixels).max(top_band_end);

    for y in clip.top..clip.bottom {
        let row_start = (y * width) as usize;
        let row = &mut pixels[row_start..row_start + width as usize];
        if y < border_top || y >= border_bottom {
            dim_pixels(&mut row[clip.left as usize..clip.right as usize], inv_dim);
        } else if y >= top_band_end && y < bottom_band_start {
            render_middle_row(row, clip, selection, inv_dim);
        } else {
            let geometry = CornerGeometry {
                border_x: (border_left, border_right),
                center: (center_x, center_y),
                half_size: (half_width, half_height),
                radius,
                dim_f,
                inv_dim,
            };
            render_corner_row(row, y, clip, &geometry);
        }
    }
}

fn render_middle_row(row: &mut [u32], clip: RECT, selection: RECT, inv_dim: u32) {
    let left_end = clip.right.min(selection.left).max(clip.left);
    dim_pixels(&mut row[clip.left as usize..left_end as usize], inv_dim);

    let clear_left = clip.left.max(selection.left).min(clip.right);
    let clear_right = clip.right.min(selection.right).max(clear_left);
    fix_alpha(&mut row[clear_left as usize..clear_right as usize]);
    for offset in 0..2 {
        for x in [selection.left + offset, selection.right - 1 - offset] {
            if x >= clip.left && x < clip.right && x >= 0 && x < row.len() as i32 {
                row[x as usize] = 0xffff_ffff;
            }
        }
    }

    let right_start = clip.left.max(selection.right).min(clip.right);
    dim_pixels(&mut row[right_start as usize..clip.right as usize], inv_dim);
}

fn render_corner_row(row: &mut [u32], y: i32, clip: RECT, geometry: &CornerGeometry) {
    let sdf_left = clip.left.max(geometry.border_x.0).min(clip.right);
    let sdf_right = clip.right.min(geometry.border_x.1).max(sdf_left);
    dim_pixels(
        &mut row[clip.left as usize..sdf_left as usize],
        geometry.inv_dim,
    );

    let py = y as f32 + 0.5;
    for x in sdf_left..sdf_right {
        let px = x as f32 + 0.5;
        let dx = (px - geometry.center.0).abs() - (geometry.half_size.0 - geometry.radius);
        let dy = (py - geometry.center.1).abs() - (geometry.half_size.1 - geometry.radius);
        let distance = if dx > 0.0 && dy > 0.0 {
            (dx * dx + dy * dy).sqrt() - geometry.radius
        } else {
            dx.max(dy) - geometry.radius
        };
        let outer = (0.5 - distance).clamp(0.0, 1.0);
        let inner = (0.5 - (distance + 2.0)).clamp(0.0, 1.0);
        let border = outer - inner;
        let inverse_dim = 1.0 - geometry.dim_f * (1.0 - outer);
        let inverse_border = 1.0 - border;
        let value = row[x as usize];
        let blend = |channel: u32| {
            ((channel as f32 * inverse_dim * inverse_border + 255.0 * border) as u32).min(255)
        };
        row[x as usize] = 0xff00_0000
            | (blend((value >> 16) & 0xff) << 16)
            | (blend((value >> 8) & 0xff) << 8)
            | blend(value & 0xff);
    }

    dim_pixels(
        &mut row[sdf_right as usize..clip.right as usize],
        geometry.inv_dim,
    );
}

fn dim_rect(pixels: &mut [u32], width: i32, rect: RECT, inv_dim: u32) {
    for y in rect.top..rect.bottom {
        let row_start = (y * width) as usize;
        dim_pixels(
            &mut pixels[row_start + rect.left as usize..row_start + rect.right as usize],
            inv_dim,
        );
    }
}
