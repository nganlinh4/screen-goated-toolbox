use base64::Engine as _;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

pub(crate) fn capture_window_thumbnail(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut rect).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(hwnd, &mut rect).is_err()
        {
            return None;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return None;
        }
        let scale = (250.0 / width.max(height) as f32).min(1.0);
        let target_width = ((width as f32 * scale).round() as i32).max(1);
        let target_height = ((height as f32 * scale).round() as i32).max(1);

        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let source_dc = CreateCompatibleDC(Some(screen));
        if source_dc.is_invalid() {
            let _ = ReleaseDC(None, screen);
            return None;
        }
        let source_bitmap = CreateCompatibleBitmap(screen, width, height);
        if source_bitmap.0.is_null() {
            let _ = DeleteDC(source_dc);
            let _ = ReleaseDC(None, screen);
            return None;
        }
        let old_source = SelectObject(source_dc, source_bitmap.into());
        if !PrintWindow(hwnd, source_dc, PRINT_WINDOW_FLAGS(2)).as_bool() {
            cleanup_bitmap_dc(source_dc, source_bitmap, old_source);
            let _ = ReleaseDC(None, screen);
            return None;
        }

        let target_dc = CreateCompatibleDC(Some(screen));
        let target_bitmap = CreateCompatibleBitmap(screen, target_width, target_height);
        if target_dc.is_invalid() || target_bitmap.0.is_null() {
            if !target_dc.is_invalid() {
                let _ = DeleteDC(target_dc);
            }
            if !target_bitmap.0.is_null() {
                let _ = DeleteObject(target_bitmap.into());
            }
            cleanup_bitmap_dc(source_dc, source_bitmap, old_source);
            let _ = ReleaseDC(None, screen);
            return None;
        }
        let old_target = SelectObject(target_dc, target_bitmap.into());
        let _ = SetStretchBltMode(target_dc, HALFTONE);
        let _ = StretchBlt(
            target_dc,
            0,
            0,
            target_width,
            target_height,
            Some(source_dc),
            0,
            0,
            width,
            height,
            SRCCOPY,
        );

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: target_width,
                biHeight: -target_height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0_u8; (target_width * target_height * 4) as usize];
        let lines = GetDIBits(
            target_dc,
            target_bitmap,
            0,
            target_height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        cleanup_bitmap_dc(target_dc, target_bitmap, old_target);
        cleanup_bitmap_dc(source_dc, source_bitmap, old_source);
        let _ = ReleaseDC(None, screen);
        if lines == 0 {
            return None;
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let image = image::RgbaImage::from_raw(
            target_width as u32,
            target_height as u32,
            pixels,
        )?;
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 75)
            .encode_image(&image::DynamicImage::ImageRgba8(image))
            .ok()?;
        Some(format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(jpeg)
        ))
    }
}

unsafe fn cleanup_bitmap_dc(dc: HDC, bitmap: HBITMAP, previous: HGDIOBJ) {
    unsafe {
        let _ = SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
}
