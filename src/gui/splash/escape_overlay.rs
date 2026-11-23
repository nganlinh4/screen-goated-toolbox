// --- ESCAPE OVERLAY ---
// Fullscreen transparent layered window for voxels that fly beyond the main window.
// Uses a software-rendered BGRA bitmap with UpdateLayeredWindow. Voxels are drawn
// as whole cells of the splash's pixel grid, extended past the window edge.

use std::mem::size_of;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct EscapeCircle {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// The splash pixel grid in overlay-local physical pixels: cell `(i, j)` covers
/// `origin + (i, j) * cell` for one `cell` square.
#[derive(Clone, Copy)]
pub struct EscapeGrid {
    pub origin_x: i32,
    pub origin_y: i32,
    pub cell: i32,
}

pub struct EscapeOverlay {
    hwnd: HWND,
    hdc_mem: HDC,
    hbmp: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *mut u8,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: i32,
    pub height: i32,
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl EscapeOverlay {
    pub fn new() -> Option<Self> {
        unsafe {
            let class_name = windows::core::w!("SGTEscapeOverlay");
            let h_inst = GetModuleHandleW(None).ok()?;

            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(overlay_wnd_proc),
                hInstance: h_inst.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            // 1px shorter so Windows doesn't detect a fullscreen topmost app and auto-hide the taskbar
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN) - 1;

            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                class_name,
                windows::core::w!(""),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(HINSTANCE(h_inst.0)),
                None,
            )
            .ok()?;

            let hdc_screen = GetDC(None);
            if hdc_screen.is_invalid() {
                let _ = DestroyWindow(hwnd);
                return None;
            }
            let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
            let _ = ReleaseDC(None, hdc_screen);
            if hdc_mem.is_invalid() {
                let _ = DestroyWindow(hwnd);
                return None;
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbmp =
                match CreateDIBSection(Some(hdc_mem), &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0)
                    .ok()
                {
                    Some(hbmp) => hbmp,
                    None => {
                        let _ = DeleteDC(hdc_mem);
                        let _ = DestroyWindow(hwnd);
                        return None;
                    }
                };
            let old_bmp = SelectObject(hdc_mem, hbmp.into());

            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            Some(Self {
                hwnd,
                hdc_mem,
                hbmp,
                old_bmp,
                bits: bits_ptr as *mut u8,
                origin_x: x,
                origin_y: y,
                width,
                height,
            })
        }
    }

    pub fn update(&self, circles: &[EscapeCircle], grid: EscapeGrid) {
        let buf_len = (self.width * self.height * 4) as usize;
        let buffer = unsafe { std::slice::from_raw_parts_mut(self.bits, buf_len) };

        buffer.fill(0);

        for c in circles {
            draw_cells(buffer, self.width, self.height, c, grid);
        }

        unsafe {
            let pt_pos = POINT {
                x: self.origin_x,
                y: self.origin_y,
            };
            let pt_size = SIZE {
                cx: self.width,
                cy: self.height,
            };
            let pt_src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&pt_pos),
                Some(&pt_size),
                Some(self.hdc_mem),
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }
}

impl Drop for EscapeOverlay {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc_mem, self.old_bmp);
            let _ = DeleteObject(self.hbmp.into());
            let _ = DeleteDC(self.hdc_mem);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Fills every grid cell whose centre lies inside the circle, the same rule the
/// framebuffer's `fill_disc` uses. A circle smaller than a cell fills the one
/// cell containing its centre.
fn draw_cells(buffer: &mut [u8], buf_w: i32, buf_h: i32, c: &EscapeCircle, grid: EscapeGrid) {
    if c.radius <= 0.0 || c.a == 0 || grid.cell <= 0 {
        return;
    }
    let size = grid.cell as f32;
    let col = |x: f32| ((x - grid.origin_x as f32) / size).floor() as i32;
    let row = |y: f32| ((y - grid.origin_y as f32) / size).floor() as i32;
    if c.radius < 0.8 * size {
        fill_cell(buffer, buf_w, buf_h, grid, col(c.x), row(c.y), c);
        return;
    }
    let r2 = c.radius * c.radius;
    // Only the cells that overlap both the circle and the bitmap.
    let cols = col(c.x - c.radius).max(col(0.0))..=col(c.x + c.radius).min(col(buf_w as f32));
    let rows = row(c.y - c.radius).max(row(0.0))..=row(c.y + c.radius).min(row(buf_h as f32));
    for j in rows {
        let dy = grid.origin_y as f32 + (j as f32 + 0.5) * size - c.y;
        for i in cols.clone() {
            let dx = grid.origin_x as f32 + (i as f32 + 0.5) * size - c.x;
            if dx * dx + dy * dy <= r2 {
                fill_cell(buffer, buf_w, buf_h, grid, i, j, c);
            }
        }
    }
}

/// Composites one solid cell (premultiplied BGRA), clipped to the bitmap.
fn fill_cell(
    buffer: &mut [u8],
    buf_w: i32,
    buf_h: i32,
    grid: EscapeGrid,
    i: i32,
    j: i32,
    c: &EscapeCircle,
) {
    let x0 = grid.origin_x.saturating_add(i.saturating_mul(grid.cell));
    let y0 = grid.origin_y.saturating_add(j.saturating_mul(grid.cell));
    let (x_start, x_end) = (x0.max(0), x0.saturating_add(grid.cell).min(buf_w));
    let (y_start, y_end) = (y0.max(0), y0.saturating_add(grid.cell).min(buf_h));
    let sa = c.a as f32 / 255.0;
    let inv = 1.0 - sa;
    let src = [c.b, c.g, c.r].map(|v| v as f32 * sa);
    for py in y_start..y_end {
        let row = (py * buf_w * 4) as usize;
        for px in x_start..x_end {
            let idx = row + (px * 4) as usize;
            for (channel, value) in src.iter().enumerate() {
                let dst = &mut buffer[idx + channel];
                *dst = (value + *dst as f32 * inv).min(255.0) as u8;
            }
            buffer[idx + 3] = (sa * 255.0 + buffer[idx + 3] as f32 * inv).min(255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: i32 = 64;
    const H: i32 = 48;
    const GRID: EscapeGrid = EscapeGrid {
        origin_x: 2,
        origin_y: -1,
        cell: 4,
    };

    fn circle(x: f32, y: f32, radius: f32) -> EscapeCircle {
        EscapeCircle {
            x,
            y,
            radius,
            r: 200,
            g: 100,
            b: 50,
            a: 255,
        }
    }

    fn painted(buffer: &[u8], x: i32, y: i32) -> bool {
        buffer[((y * W + x) * 4 + 3) as usize] != 0
    }

    #[test]
    fn escaped_voxels_fill_whole_grid_cells_only() {
        let mut buffer = vec![0u8; (W * H * 4) as usize];
        draw_cells(&mut buffer, W, H, &circle(30.3, 21.7, 9.0), GRID);
        let mut any = false;
        for y in 0..H {
            for x in 0..W {
                let (i, j) = (
                    (x - GRID.origin_x).div_euclid(4),
                    (y - GRID.origin_y).div_euclid(4),
                );
                let cell_x = GRID.origin_x + i * 4;
                let cell_y = GRID.origin_y + j * 4;
                // Every pixel matches the rest of its cell: no partial-coverage edge.
                let reference = painted(&buffer, cell_x.clamp(0, W - 1), cell_y.clamp(0, H - 1));
                assert_eq!(painted(&buffer, x, y), reference, "pixel {x},{y}");
                any |= reference;
            }
        }
        assert!(any);
        // Solid colour, not an anti-aliased blend.
        let centre = ((21 * W + 30) * 4) as usize;
        assert_eq!(&buffer[centre..centre + 4], &[50, 100, 200, 255]);
    }

    #[test]
    fn a_voxel_smaller_than_a_cell_fills_the_cell_it_sits_in() {
        let mut buffer = vec![0u8; (W * H * 4) as usize];
        draw_cells(&mut buffer, W, H, &circle(11.0, 9.5, 1.0), GRID);
        // Cell (2, 2) spans x 10..14, y 7..11.
        for y in 0..H {
            for x in 0..W {
                assert_eq!(
                    painted(&buffer, x, y),
                    (10..14).contains(&x) && (7..11).contains(&y),
                    "pixel {x},{y}"
                );
            }
        }
    }

    #[test]
    fn cells_past_the_bitmap_edge_are_clipped() {
        let mut buffer = vec![0u8; (W * H * 4) as usize];
        draw_cells(&mut buffer, W, H, &circle(-3.0, 46.0, 12.0), GRID);
        draw_cells(&mut buffer, W, H, &circle(1.0e9, -1.0e9, 5.0), GRID);
        assert!(painted(&buffer, 0, H - 1));
    }
}
