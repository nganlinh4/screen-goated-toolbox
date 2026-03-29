// --- PAINT HELPERS ---
// Math helpers and bitmap utilities for result overlay painting.

use std::mem::size_of;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::core::w;

// --- TEXT MEASUREMENT ---

pub unsafe fn measure_text_bounds(
    hdc: HDC,
    text: &mut [u16],
    font_size: i32,
    max_width: i32,
) -> (i32, i32) {
    unsafe {
        let hfont = CreateFontW(
            font_size,
            0,
            0,
            0,
            FW_MEDIUM.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
            w!("Google Sans Flex"),
        );
        let old_font = SelectObject(hdc, hfont.into());

        let mut calc_rect = RECT {
            left: 0,
            top: 0,
            right: max_width,
            bottom: 0,
        };

        DrawTextW(
            hdc,
            text,
            &mut calc_rect,
            DT_CALCRECT | DT_WORDBREAK | DT_EDITCONTROL,
        );

        SelectObject(hdc, old_font);
        let _ = DeleteObject(hfont.into());

        // Return (Height, Width)
        (calc_rect.bottom, calc_rect.right)
    }
}

// --- BITMAP CREATION ---

pub fn create_bitmap_from_pixels(pixels: &[u32], w: i32, h: i32) -> HBITMAP {
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return HBITMAP::default();
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbm = match CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(hbm) => hbm,
            Err(_) => {
                let _ = ReleaseDC(None, hdc);
                return HBITMAP::default();
            }
        };
        if !bits.is_null() {
            std::ptr::copy_nonoverlapping(
                pixels.as_ptr() as *const u8,
                bits as *mut u8,
                pixels.len() * 4,
            );
        }
        let _ = ReleaseDC(None, hdc);
        hbm
    }
}
