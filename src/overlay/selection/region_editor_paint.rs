use super::region_editor::{controls, handles, with_editor};
use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::*};

pub(super) fn paint(pixels: &mut [u32], width: i32, height: i32, alpha: u8) {
    with_editor(|s| {
        crate::overlay::image_continuous_mode::dim_pixels(pixels, 256 - u32::from(alpha) / 3);
        let ox = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let oy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let fade = (f32::from(alpha) / f32::from(super::state::TARGET_OPACITY)).min(1.0);
        let mut blend = |x: i32, y: i32, color: u32, a: f32| {
            let (x, y) = (x - ox, y - oy);
            if x < 0 || y < 0 || x >= width || y >= height {
                return;
            }
            let value = &mut pixels[(y * width + x) as usize];
            let a = (a * fade).clamp(0.0, 1.0);
            let channel = |shift: u32| {
                ((((*value >> shift) & 255u32) as f32 * (1.0 - a)
                    + ((color >> shift) & 255u32) as f32 * a)
                    .round() as u32)
                    << shift
            };
            *value = 0xff000000u32 | channel(0) | channel(8) | channel(16);
        };
        let radius = (5.0 * s.scale).round() as i32;
        for p in handles(s.rect, s.bounds, (6.0 * s.scale).round() as i32) {
            for y in p.y - radius..=p.y + radius {
                for x in p.x - radius..=p.x + radius {
                    let border = (x - p.x).abs() == radius || (y - p.y).abs() == radius;
                    blend(x, y, if border { 0x4568c8 } else { 0xffffff }, 1.0);
                }
            }
        }
        for (i, r) in controls(s).iter().enumerate() {
            rounded(
                *r,
                (8.0 * s.scale).round() as i32,
                if s.hover == Some(i + 9) {
                    0x424653
                } else {
                    0x20232b
                },
                &mut blend,
            );
            let (side, mask) = &s.icons[i];
            let left = (r.left + r.right - *side as i32) / 2;
            let top = (r.top + r.bottom - *side as i32) / 2;
            for y in 0..*side {
                for x in 0..*side {
                    blend(
                        left + x as i32,
                        top + y as i32,
                        0xffffff,
                        f32::from(mask[y * side + x]) / 255.0,
                    );
                }
            }
        }
    });
}
fn rounded(r: RECT, radius: i32, color: u32, blend: &mut impl FnMut(i32, i32, u32, f32)) {
    for y in r.top..r.bottom {
        for x in r.left..r.right {
            let dx = (r.left + radius - x).max(x - (r.right - radius - 1)).max(0);
            let dy = (r.top + radius - y).max(y - (r.bottom - radius - 1)).max(0);
            let coverage =
                (radius as f32 + 0.5 - ((dx * dx + dy * dy) as f32).sqrt()).clamp(0.0, 1.0);
            blend(x, y, color, coverage);
        }
    }
}
