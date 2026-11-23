//! Best-effort recovery of GPU-composited foreground content that GDI captured
//! as black.
//!
//! `PrintWindow(PW_RENDERFULLCONTENT)` renders the foreground window as if
//! nothing covered it, so its pixels are only patched where that window is
//! actually on top: every pixel covered by a window above it in z-order keeps
//! the desktop capture. The render bitmap covers only the window's visible
//! part of the capture, so an oversized window cannot force a huge allocation.

use windows::Win32::Foundation::{HWND, RECT};

/// A rectangle in capture-image pixels, `right`/`bottom` exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Area {
    fn from_screen(rect: RECT, origin_x: i32, origin_y: i32) -> Self {
        Self {
            left: rect.left - origin_x,
            top: rect.top - origin_y,
            right: rect.right - origin_x,
            bottom: rect.bottom - origin_y,
        }
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let area = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (area.left < area.right && area.top < area.bottom).then_some(area)
    }

    fn contains(self, x: i32, y: i32) -> bool {
        (self.left..self.right).contains(&x) && (self.top..self.bottom).contains(&y)
    }

    fn width(self) -> i32 {
        self.right - self.left
    }

    fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Largest render bitmap, as a multiple of the capture's own pixel count.
const MAX_RENDER_TO_CAPTURE: i64 = 2;

/// The bitmap PrintWindow draws into: it starts at the window's top-left
/// (PrintWindow always renders from there) and stops at the visible part's
/// bottom-right, so nothing past the capture is allocated. A window hanging
/// far off the top or left edge is skipped rather than rendered in full.
fn render_extent(window: Area, visible: Area, image: Area) -> Option<(i32, i32)> {
    let (width, height) = (visible.right - window.left, visible.bottom - window.top);
    let budget = image.width() as i64 * image.height() as i64 * MAX_RENDER_TO_CAPTURE;
    (width as i64 * height as i64 <= budget).then_some((width, height))
}

/// Copy rendered pixels onto the capture where the window is the top window
/// and the render is not itself black.
fn apply_patch(
    rgb: &mut image::RgbImage,
    bgra: &[u8],
    stride: i32,
    window: Area,
    visible: Area,
    occluders: &[Area],
) {
    for y in visible.top..visible.bottom {
        for x in visible.left..visible.right {
            if occluders.iter().any(|area| area.contains(x, y)) {
                continue;
            }
            let idx = (((y - window.top) * stride + (x - window.left)) * 4) as usize;
            let Some(&[b, g, r, _]) = bgra.get(idx..idx + 4) else {
                continue;
            };
            if b as u32 + g as u32 + r as u32 > 12 {
                rgb.put_pixel(x as u32, y as u32, image::Rgb([r, g, b]));
            }
        }
    }
}

/// Visible windows above `hwnd` in z-order that receive input, clipped to
/// `bounds`. Click-through layered overlays let clicks reach the window
/// below, so they do not hide it.
fn occluders_above(hwnd: HWND, origin_x: i32, origin_y: i32, bounds: Area) -> Vec<Area> {
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::UI::WindowsAndMessaging::{
        GW_HWNDPREV, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowRect, IsIconic,
        IsWindowVisible, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };
    let click_through = (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as i32;
    let mut areas = Vec::new();
    let mut current = hwnd;
    // A z-order walk is bounded by the number of top-level windows; the cap
    // only guards against a list that keeps changing under the walk.
    for _ in 0..4096 {
        let Ok(above) = (unsafe { GetWindow(current, GW_HWNDPREV) }) else {
            break;
        };
        if above.is_invalid() {
            break;
        }
        current = above;
        let shown = unsafe { IsWindowVisible(above).as_bool() && !IsIconic(above).as_bool() };
        let ex_style = unsafe { GetWindowLongW(above, GWL_EXSTYLE) };
        if !shown || ex_style & click_through == click_through {
            continue;
        }
        let mut cloaked = 0u32;
        let cloak_known = unsafe {
            DwmGetWindowAttribute(
                above,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        }
        .is_ok();
        if cloak_known && cloaked != 0 {
            continue;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(above, &mut rect) }.is_ok()
            && let Some(area) = Area::from_screen(rect, origin_x, origin_y).intersect(bounds)
        {
            areas.push(area);
        }
    }
    areas
}

/// When the FOREGROUND window's region in the GDI capture is near-black (GDI
/// missed its GPU content), re-render just that window and patch its pixels in
/// where it is the top window. Guarded so it can only ADD detail where GDI
/// failed; pure hardware-overlay WebGL may still be black (that needs WGC).
pub(super) fn patch_foreground_gpu(rgb: &mut image::RgbImage, origin_x: i32, origin_y: i32) {
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};
    const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(0x0000_0002);

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let window = Area::from_screen(rect, origin_x, origin_y);
    let image = Area {
        left: 0,
        top: 0,
        right: rgb.width() as i32,
        bottom: rgb.height() as i32,
    };
    let Some(visible) = window.intersect(image) else {
        return;
    };
    if super::region_avg(
        rgb,
        visible.left,
        visible.top,
        visible.width(),
        visible.height(),
    ) >= 24
    {
        return; // GDI already captured it (not black) - nothing to fix
    }
    let Some((width, height)) = render_extent(window, visible, image) else {
        return;
    };
    let occluders = occluders_above(hwnd, origin_x, origin_y, visible);
    let mut bgra = vec![0u8; width as usize * height as usize * 4];
    let rendered = unsafe {
        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
        let old = SelectObject(hdc_mem, hbm.into());
        let mut rendered = PrintWindow(hwnd, hdc_mem, PW_RENDERFULLCONTENT).as_bool();
        if rendered {
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            rendered = GetDIBits(
                hdc_mem,
                hbm,
                0,
                height as u32,
                Some(bgra.as_mut_ptr().cast()),
                &mut bmi,
                DIB_RGB_COLORS,
            ) == height;
        }
        let _ = SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbm.into());
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
        rendered
    };
    if rendered {
        apply_patch(rgb, &bgra, width, window, visible, &occluders);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(left: i32, top: i32, right: i32, bottom: i32) -> Area {
        Area {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn render_bitmap_stops_at_the_visible_capture() {
        let image = area(0, 0, 1920, 1080);
        let huge = area(-100, -50, 16_000, 16_000);
        let visible = huge.intersect(image).unwrap();
        assert_eq!(visible, area(0, 0, 1920, 1080));
        assert_eq!(render_extent(huge, visible, image), Some((2020, 1130)));
        let far_left = area(-16_000, 0, 1_000, 1_000);
        let visible = far_left.intersect(image).unwrap();
        assert_eq!(render_extent(far_left, visible, image), None);
        assert!(area(2000, 0, 2100, 50).intersect(image).is_none());
    }

    #[test]
    #[ignore = "desktop: walks the live z-order above the foreground window (read-only)"]
    fn live_occluder_walk_stays_inside_its_bounds() {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let bounds = area(0, 0, 1920, 1080);
        let hwnd = unsafe { GetForegroundWindow() };
        let occluders = occluders_above(hwnd, 0, 0, bounds);
        assert!(
            occluders
                .iter()
                .all(|found| found.intersect(bounds) == Some(*found))
        );
        eprintln!("occluders above the foreground window: {occluders:?}");
    }

    #[test]
    fn covered_pixels_keep_the_desktop_capture() {
        let mut rgb = image::RgbImage::from_pixel(4, 1, image::Rgb([0, 0, 0]));
        let window = area(0, 0, 4, 1);
        // Rendered window: bright blue everywhere except a black last pixel.
        let mut bgra = [200u8, 0, 0, 255].repeat(4);
        bgra[12..16].copy_from_slice(&[0, 0, 0, 255]);
        let topmost = [area(1, 0, 3, 1)];
        rgb.put_pixel(1, 0, image::Rgb([9, 9, 9]));
        apply_patch(&mut rgb, &bgra, 4, window, window, &topmost);
        let pixels: Vec<_> = rgb.pixels().map(|pixel| pixel.0).collect();
        assert_eq!(
            pixels,
            [[0, 0, 200], [9, 9, 9], [0, 0, 0], [0, 0, 0]],
            "only the uncovered, non-black pixel is patched"
        );
    }
}
