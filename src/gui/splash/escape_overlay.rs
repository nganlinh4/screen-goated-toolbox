// --- ESCAPE OVERLAY ---
// Fullscreen transparent layered window for voxels that fly beyond the main window.
// Uses a software-rendered BGRA bitmap with UpdateLayeredWindow.

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
    DefWindowProcW(hwnd, msg, wparam, lparam)
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
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

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
            let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
            ReleaseDC(None, hdc_screen);

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
            let hbmp = CreateDIBSection(
                Some(hdc_mem),
                &bmi,
                DIB_RGB_COLORS,
                &mut bits_ptr,
                None,
                0,
            )
            .ok()?;
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

    pub fn update(&self, circles: &[EscapeCircle]) {
        let buf_len = (self.width * self.height * 4) as usize;
        let buffer = unsafe { std::slice::from_raw_parts_mut(self.bits, buf_len) };

        buffer.fill(0);

        for c in circles {
            draw_circle(buffer, self.width, self.height, c);
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

fn draw_circle(buffer: &mut [u8], buf_w: i32, buf_h: i32, c: &EscapeCircle) {
    if c.radius < 0.5 || c.a == 0 {
        return;
    }

    let x0 = (c.x - c.radius - 1.0).max(0.0) as i32;
    let y0 = (c.y - c.radius - 1.0).max(0.0) as i32;
    let x1 = ((c.x + c.radius + 1.0) as i32).min(buf_w - 1);
    let y1 = ((c.y + c.radius + 1.0) as i32).min(buf_h - 1);

    let rf = c.r as f32 / 255.0;
    let gf = c.g as f32 / 255.0;
    let bf = c.b as f32 / 255.0;
    let af = c.a as f32 / 255.0;
    let outer_sq = (c.radius + 1.0) * (c.radius + 1.0);
    let inner_sq = (c.radius - 0.5).max(0.0) * (c.radius - 0.5).max(0.0);

    for py in y0..=y1 {
        let dy = py as f32 + 0.5 - c.y;
        let dy_sq = dy * dy;
        let row = (py * buf_w * 4) as usize;

        for px in x0..=x1 {
            let dx = px as f32 + 0.5 - c.x;
            let dist_sq = dx * dx + dy_sq;

            if dist_sq > outer_sq {
                continue;
            }

            let coverage = if dist_sq <= inner_sq {
                1.0
            } else {
                let dist = dist_sq.sqrt();
                (c.radius + 0.5 - dist).clamp(0.0, 1.0)
            };

            let sa = af * coverage;
            if sa < 0.004 {
                continue;
            }

            let idx = row + (px * 4) as usize;
            let inv = 1.0 - sa;

            // Premultiplied alpha composite (BGRA order for Windows DIB)
            buffer[idx] = (bf * sa * 255.0 + buffer[idx] as f32 * inv).min(255.0) as u8;
            buffer[idx + 1] = (gf * sa * 255.0 + buffer[idx + 1] as f32 * inv).min(255.0) as u8;
            buffer[idx + 2] = (rf * sa * 255.0 + buffer[idx + 2] as f32 * inv).min(255.0) as u8;
            buffer[idx + 3] = (sa * 255.0 + buffer[idx + 3] as f32 * inv).min(255.0) as u8;
        }
    }
}
