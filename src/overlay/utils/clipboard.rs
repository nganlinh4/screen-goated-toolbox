use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    BITMAPINFO, BITMAPINFOHEADER, CBM_INIT, CreateDIBitmap, DIB_RGB_COLORS, GetDC, ReleaseDC,
};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;

// --- CLIPBOARD SUPPORT ---
pub fn copy_to_clipboard(text: &str, hwnd: HWND) {
    unsafe {
        // Retry loop to handle temporary clipboard locks
        for attempt in 0..5 {
            if OpenClipboard(Some(hwnd)).is_ok() {
                let _ = EmptyClipboard();

                // Convert text to UTF-16
                let wide_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let mem_size = wide_text.len() * 2;

                // Allocate global memory
                if let Ok(h_mem) = GlobalAlloc(GMEM_MOVEABLE, mem_size) {
                    let ptr = GlobalLock(h_mem) as *mut u16;
                    std::ptr::copy_nonoverlapping(wide_text.as_ptr(), ptr, wide_text.len());
                    let _ = GlobalUnlock(h_mem);

                    // Set clipboard data (CF_UNICODETEXT = 13)
                    let h_mem_handle = HANDLE(h_mem.0);
                    let _ = SetClipboardData(13u32, Some(h_mem_handle));
                }

                let _ = CloseClipboard();
                return; // Success
            }

            // If failed and not last attempt, wait before retrying
            if attempt < 4 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            } else {
                eprintln!("Failed to copy to clipboard after 5 attempts");
            }
        }
    }
}

pub fn copy_image_to_clipboard(image_bytes: &[u8]) {
    // Convert PNG/etc bytes to BMP format using image crate
    // Clipboard expects CF_DIB which is BMP without the File Header (first 14 bytes)
    if let Ok(img) = image::load_from_memory(image_bytes) {
        let mut bmp_data = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut bmp_data);
        if img.write_to(&mut cursor, image::ImageFormat::Bmp).is_ok() {
            // Check if valid BMP (starts with BM)
            if bmp_data.len() > 14 && bmp_data[0] == 0x42 && bmp_data[1] == 0x4D {
                // Skip the 14-byte BITMAPFILEHEADER to get BITMAPINFOHEADER + Pixels (DIB)
                let dib_data = &bmp_data[14..];

                unsafe {
                    // Retry loop
                    for attempt in 0..5 {
                        if OpenClipboard(None).is_ok() {
                            let _ = EmptyClipboard();

                            let mem_size = dib_data.len();
                            if let Ok(h_mem) = GlobalAlloc(GMEM_MOVEABLE, mem_size) {
                                let ptr = GlobalLock(h_mem) as *mut u8;
                                std::ptr::copy_nonoverlapping(dib_data.as_ptr(), ptr, mem_size);
                                let _ = GlobalUnlock(h_mem);

                                // Set CF_DIB (8)
                                let h_mem_handle = HANDLE(h_mem.0);
                                let _ = SetClipboardData(8, Some(h_mem_handle));

                                // ALSO set CF_BITMAP (2) to ensure Windows Clipboard History picks it up.
                                // Many modern Windows apps/features prefer having a GDI handle or both formats.
                                {
                                    let hdc = GetDC(None);
                                    if !hdc.is_invalid() {
                                        // Read header size (first 4 bytes of DIB data)
                                        if dib_data.len() >= 4 {
                                            let header_size = u32::from_le_bytes(
                                                dib_data[0..4].try_into().unwrap_or([0; 4]),
                                            );
                                            // The bits usually start after the header.
                                            // Make sure we don't go out of bounds.
                                            if (header_size as usize) < dib_data.len() {
                                                let bits_ptr =
                                                    dib_data.as_ptr().add(header_size as usize);
                                                let pbmih =
                                                    dib_data.as_ptr() as *const BITMAPINFOHEADER;
                                                let pbmi = dib_data.as_ptr() as *const BITMAPINFO;

                                                let hbitmap = CreateDIBitmap(
                                                    hdc,
                                                    Some(pbmih),
                                                    CBM_INIT as u32,
                                                    Some(bits_ptr as *const std::ffi::c_void),
                                                    Some(pbmi),
                                                    DIB_RGB_COLORS,
                                                );

                                                if !hbitmap.is_invalid() {
                                                    // ownership transferred to system
                                                    let _ = SetClipboardData(
                                                        2, // CF_BITMAP
                                                        Some(HANDLE(hbitmap.0 as *mut _)),
                                                    );
                                                }
                                            }
                                        }
                                        ReleaseDC(None, hdc);
                                    }
                                }

                                let _ = CloseClipboard();
                                return;
                            }
                        }
                        if attempt < 4 {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                }
            }
        }
    }
}

/// Read image bytes from clipboard (returns PNG-encoded bytes)
/// Returns None if no image is available in clipboard
pub fn get_clipboard_image_bytes() -> Option<Vec<u8>> {
    use windows::Win32::System::DataExchange::IsClipboardFormatAvailable;

    unsafe {
        // Try to open clipboard with retries
        for _attempt in 0..5 {
            if OpenClipboard(None).is_ok() {
                // Check if any image format is available
                // CF_DIB = 8, CF_DIBV5 = 17
                let has_dib = IsClipboardFormatAvailable(8).is_ok();
                let has_dibv5 = IsClipboardFormatAvailable(17).is_ok();

                if !has_dib && !has_dibv5 {
                    let _ = CloseClipboard();
                    return None;
                }

                // Try CF_DIB first (8), then CF_DIBV5 (17)
                let format_to_try = if has_dib { 8u32 } else { 17u32 };

                if let Ok(h_data) = GetClipboardData(format_to_try) {
                    let ptr = GlobalLock(HGLOBAL(h_data.0));
                    if !ptr.is_null() {
                        // Get the size of the global memory block
                        let size = GlobalSize(HGLOBAL(h_data.0));
                        if size > 0 {
                            // Read DIB data
                            let dib_data = std::slice::from_raw_parts(ptr as *const u8, size);

                            // Parse BITMAPINFOHEADER to get dimensions
                            if dib_data.len() >= std::mem::size_of::<BITMAPINFOHEADER>() {
                                let header = &*(dib_data.as_ptr() as *const BITMAPINFOHEADER);
                                let width = header.biWidth;
                                let height = header.biHeight.abs();
                                let bit_count = header.biBitCount;
                                let is_top_down = header.biHeight < 0;

                                // We support 24-bit and 32-bit images
                                if (bit_count == 24 || bit_count == 32) && width > 0 && height > 0 {
                                    // Calculate the offset to pixel data
                                    let header_size = header.biSize as usize;
                                    let color_table_size = if header.biClrUsed > 0 {
                                        (header.biClrUsed as usize) * 4
                                    } else if bit_count <= 8 {
                                        (1 << bit_count) * 4
                                    } else {
                                        0
                                    };
                                    let pixel_offset = header_size + color_table_size;

                                    if dib_data.len() > pixel_offset {
                                        let pixel_data = &dib_data[pixel_offset..];
                                        let bytes_per_pixel = (bit_count / 8) as usize;
                                        let row_size =
                                            (width as usize * bytes_per_pixel).div_ceil(4) * 4; // DWORD aligned

                                        // Create RGBA buffer
                                        let mut rgba_buffer =
                                            Vec::with_capacity((width * height * 4) as usize);

                                        for y in 0..height {
                                            let src_y =
                                                if is_top_down { y } else { height - 1 - y };
                                            let row_start = (src_y as usize) * row_size;

                                            for x in 0..width {
                                                let px_start =
                                                    row_start + (x as usize) * bytes_per_pixel;
                                                if px_start + bytes_per_pixel <= pixel_data.len() {
                                                    // DIB is BGR(A)
                                                    let b = pixel_data[px_start];
                                                    let g = pixel_data[px_start + 1];
                                                    let r = pixel_data[px_start + 2];
                                                    let a = if bytes_per_pixel == 4 {
                                                        pixel_data[px_start + 3]
                                                    } else {
                                                        255
                                                    };
                                                    rgba_buffer.push(r);
                                                    rgba_buffer.push(g);
                                                    rgba_buffer.push(b);
                                                    rgba_buffer.push(a);
                                                }
                                            }
                                        }

                                        let _ = GlobalUnlock(HGLOBAL(h_data.0));
                                        let _ = CloseClipboard();

                                        // Encode as PNG
                                        if let Some(img) =
                                            image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                                                width as u32,
                                                height as u32,
                                                rgba_buffer,
                                            )
                                        {
                                            let mut png_data = Vec::new();
                                            let mut cursor = std::io::Cursor::new(&mut png_data);
                                            if img
                                                .write_to(&mut cursor, image::ImageFormat::Png)
                                                .is_ok()
                                            {
                                                return Some(png_data);
                                            }
                                        }

                                        return None;
                                    }
                                }
                            }
                        }
                        let _ = GlobalUnlock(HGLOBAL(h_data.0));
                    }
                }
                let _ = CloseClipboard();
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}
