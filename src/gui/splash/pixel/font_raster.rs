//! Pixel text for scripts the authored font can't build (Korean today).
//!
//! GDI renders the line with a hinted TrueType face in bilevel mode
//! (`NONANTIALIASED_QUALITY`): the font's own hinting snaps every stroke to
//! whole pixels, with no grey edge pixels. Each rendered pixel becomes one
//! splash grid cell, so the line sits on the same grid as the authored font.

use super::font::TextBitmap;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    FF_DONTCARE, FW_NORMAL, GetTextExtentPoint32W, HGDIOBJ, NONANTIALIASED_QUALITY, OUT_TT_PRECIS,
    SelectObject, SetBkColor, SetTextColor, TextOutW,
};
use windows::core::w;

/// Character height in pixels (= cells): the smallest size at which Malgun
/// Gothic's hinting keeps every jamo stroke of a syllable separate. At 9px
/// final consonants start to break (곳), and at 8px syllables merge.
const CHARACTER_HEIGHT: i32 = 10;

/// Renders `s` and crops it to its ink. `None` if GDI fails or draws nothing.
pub(crate) fn rasterize(s: &str) -> Option<TextBitmap> {
    let text: Vec<u16> = s.encode_utf16().collect();
    if text.is_empty() {
        return None;
    }
    // SAFETY: every GDI object created here is selected out and released before
    // returning; `bits` points into the DIB section, which outlives its reads.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let font = CreateFontW(
            -CHARACTER_HEIGHT,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            NONANTIALIASED_QUALITY,
            FF_DONTCARE.0 as u32,
            w!("Malgun Gothic"),
        );
        let previous_font = SelectObject(dc, font.into());
        let mut extent = SIZE::default();
        let measured = GetTextExtentPoint32W(dc, &text, &mut extent).as_bool();
        let result = if measured && extent.cx > 0 && extent.cy > 0 {
            draw(dc, &text, extent)
        } else {
            None
        };
        SelectObject(dc, previous_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteDC(dc);
        result
    }
}

/// Draws white text on black into a 32-bit DIB and reads back the ink.
unsafe fn draw(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &[u16],
    extent: SIZE,
) -> Option<TextBitmap> {
    let (width, height) = (extent.cx, extent.cy);
    let info = BITMAPINFO {
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
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: the caller owns `dc`; the section is released below.
    let bitmap =
        unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    let previous: HGDIOBJ = unsafe { SelectObject(dc, bitmap.into()) };
    let ink = unsafe {
        SetBkColor(dc, COLORREF(0));
        SetTextColor(dc, COLORREF(0x00FF_FFFF));
        let drawn = TextOutW(dc, 0, 0, text).as_bool();
        let pixels = std::slice::from_raw_parts(bits as *const u32, (width * height) as usize);
        drawn.then(|| crop(pixels, width as usize, height as usize))
    };
    unsafe {
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
    }
    ink.flatten()
}

/// Crops a white-on-black render to its inked rectangle.
fn crop(pixels: &[u32], width: usize, height: usize) -> Option<TextBitmap> {
    let lit = |x: usize, y: usize| pixels[y * width + x] & 0x00FF_FFFF != 0;
    let rows: Vec<usize> = (0..height)
        .filter(|&y| (0..width).any(|x| lit(x, y)))
        .collect();
    let cols: Vec<usize> = (0..width)
        .filter(|&x| (0..height).any(|y| lit(x, y)))
        .collect();
    let (top, bottom) = (*rows.first()?, *rows.last()?);
    let (left, right) = (*cols.first()?, *cols.last()?);
    let (w, h) = (right - left + 1, bottom - top + 1);
    let cells = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| lit(left + x, top + y))
        .collect();
    Some(TextBitmap {
        width: w,
        height: h,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hangul_renders_as_a_bilevel_cell_bitmap() {
        let line = rasterize("번역 중...").expect("GDI renders Hangul");
        assert!((8..=14).contains(&line.height), "height {}", line.height);
        // Four syllables and three dots: far wider than tall.
        assert!(line.width > 3 * line.height, "width {}", line.width);
        // Cropped to ink on every side.
        assert!((0..line.width).any(|x| line.ink(x, 0)));
        assert!((0..line.width).any(|x| line.ink(x, line.height - 1)));
        assert!((0..line.height).any(|y| line.ink(0, y)));
        assert!((0..line.height).any(|y| line.ink(line.width - 1, y)));
    }

    #[test]
    fn crop_keeps_only_the_inked_rectangle() {
        #[rustfmt::skip]
        let pixels = [
            0, 0, 0, 0,
            0, 0xFFFFFF, 0, 0,
            0, 0, 0xFFFFFF, 0,
        ];
        let line = crop(&pixels, 4, 3).unwrap();
        assert_eq!((line.width, line.height), (2, 2));
        assert_eq!(line.cells, vec![true, false, false, true]);
        assert!(crop(&[0; 4], 2, 2).is_none());
    }
}
