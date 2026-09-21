//! Anti-aliased premultiplied surfaces for the native editor; no opaque canvas.
use super::editor_layout::Layout;
use windows::Win32::Foundation::{POINT, RECT};

pub(super) fn draw(pixels: &mut [u32], width: i32, layout: &Layout, scale: f32) {
    let origin = layout.window;
    let rounded = |pixels: &mut [u32], r: RECT, radius: f32, color: u32| {
        for y in r.top.max(origin.top)..r.bottom.min(origin.bottom) {
            for x in r.left.max(origin.left)..r.right.min(origin.right) {
                let dx = (r.left as f32 + radius - x as f32 - 0.5)
                    .max(x as f32 + 0.5 - (r.right as f32 - radius))
                    .max(0.0);
                let dy = (r.top as f32 + radius - y as f32 - 0.5)
                    .max(y as f32 + 0.5 - (r.bottom as f32 - radius))
                    .max(0.0);
                let a = if radius == 0.0 {
                    1.0
                } else {
                    (radius + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0)
                };
                let target = &mut pixels[((y - origin.top) * width + x - origin.left) as usize];
                let channel = |shift: u32| {
                    ((((color >> shift) & 255) as f32 * a
                        + ((*target >> shift) & 255) as f32 * (1.0 - a))
                        .round() as u32)
                        << shift
                };
                *target = channel(0) | channel(8) | channel(16) | channel(24);
            }
        }
    };
    for r in layout.edges {
        rounded(pixels, r, 0.0, 0xffb2d7dc);
    }
    for r in layout.handles {
        crate::overlay::region_handle::paint(
            POINT {
                x: (r.left + r.right) / 2,
                y: (r.top + r.bottom) / 2,
            },
            scale,
            |x, y, color, a| {
                if x < origin.left || x >= origin.right || y < origin.top || y >= origin.bottom {
                    return;
                }
                let target = &mut pixels[((y - origin.top) * width + x - origin.left) as usize];
                let channel = |shift: u32| {
                    ((((color >> shift) & 255) as f32 * a
                        + ((*target >> shift) & 255) as f32 * (1.0 - a))
                        .round() as u32)
                        << shift
                };
                *target = channel(0) | channel(8) | channel(16) | channel(24);
            },
        );
    }
    rounded(pixels, layout.footer, 10.0 * scale, 0xff252831);
    let inset = (4.0 * scale).round() as i32;
    rounded(
        pixels,
        RECT {
            left: layout.quit.left + inset,
            top: layout.quit.top + inset,
            right: layout.quit.right - inset,
            bottom: layout.quit.bottom - inset,
        },
        7.0 * scale,
        0xff39414a,
    );
}
