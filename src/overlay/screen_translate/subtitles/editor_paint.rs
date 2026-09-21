//! Native per-pixel layered surface, like the existing selection and bubble overlays.
use super::{View, editor_layout::Layout};
use anyhow::{Result, ensure};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

pub(super) fn paint(hwnd: HWND, view: &View, scale: f32) -> Result<()> {
    paint_checked(hwnd, view, scale, |_, _, _| {})
}

fn paint_checked(
    hwnd: HWND,
    view: &View,
    scale: f32,
    inspect: impl FnOnce(&[u32], i32, i32),
) -> Result<()> {
    let layout = Layout::new(view, scale);
    let bounds = layout.window;
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    unsafe {
        let dc = CreateCompatibleDC(None);
        ensure!(
            !dc.is_invalid(),
            "cannot create region editor drawing context"
        );
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) => bitmap,
            Err(error) => {
                let _ = DeleteDC(dc);
                return Err(error.into());
            }
        };
        let old_bitmap = SelectObject(dc, bitmap.into());
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (width * height) as usize);
        pixels.fill(0);
        let local = |r: RECT| RECT {
            left: r.left - bounds.left,
            top: r.top - bounds.top,
            right: r.right - bounds.left,
            bottom: r.bottom - bounds.top,
        };
        super::editor_surface::draw(pixels, width, &layout, scale);
        let footer = local(layout.footer);
        let quit = local(layout.quit);
        let font = super::editor_font::create(scale)?;
        let old_font = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00ffffff));
        let labels = crate::gui::locale::LocaleText::get(&view.language).screen_translate;
        let mut description: Vec<u16> = if view.error.is_empty() {
            labels.screen_translate_subtitle_description
        } else {
            &view.error
        }
        .encode_utf16()
        .collect();
        let mut description_rect = RECT {
            left: footer.left + (10.0 * scale) as i32,
            top: footer.top,
            right: quit.left - 4,
            bottom: footer.top + (28.0 * scale).round() as i32,
        };
        DrawTextW(
            dc,
            &mut description,
            &mut description_rect,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let mut label: Vec<u16> = labels
            .screen_translate_subtitle_toggle_hint
            .replace("{hotkey}", &view.hotkey)
            .encode_utf16()
            .collect();
        if !view.hotkey.is_empty() {
            SetTextColor(dc, COLORREF(0x00c4bbb0));
            let mut hint_rect = RECT {
                top: description_rect.bottom - (3.0 * scale).round() as i32,
                bottom: footer.bottom - (4.0 * scale).round() as i32,
                ..description_rect
            };
            DrawTextW(
                dc,
                &mut label,
                &mut hint_rect,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        SetTextColor(dc, COLORREF(0x00ffffff));
        let mut label: Vec<u16> = labels
            .screen_translate_subtitle_quit
            .encode_utf16()
            .collect();
        DrawTextW(
            dc,
            &mut label,
            &mut local(layout.quit),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(dc, old_font);
        let _ = DeleteObject(font.into());
        let _ = GdiFlush();
        // GDI writes RGB, not alpha. Only painted pixels become opaque; the center stays zero.
        for pixel in pixels.iter_mut() {
            if *pixel >> 24 == 0 && *pixel & 0x00ff_ffff != 0 {
                *pixel |= 0xff00_0000;
            }
        }
        inspect(pixels, width, height);
        let region = CreateRectRgn(0, 0, 0, 0);
        let region_result = (|| -> Result<()> {
            ensure!(
                !region.is_invalid(),
                "cannot create region editor input coverage"
            );
            for piece in layout.pieces() {
                let r = local(piece);
                let part = CreateRectRgn(r.left, r.top, r.right, r.bottom);
                ensure!(
                    !part.is_invalid(),
                    "cannot create region editor input segment"
                );
                let combined = CombineRgn(Some(region), Some(region), Some(part), RGN_OR);
                let _ = DeleteObject(part.into());
                ensure!(
                    combined != RGN_ERROR,
                    "cannot combine region editor input coverage"
                );
            }
            ensure!(
                SetWindowRgn(hwnd, Some(region), false) != 0,
                "cannot apply region editor input coverage"
            );
            Ok(())
        })();
        if region_result.is_err() {
            let _ = DeleteObject(region.into());
        }
        let result = region_result.and_then(|()| {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&POINT {
                    x: bounds.left,
                    y: bounds.top,
                }),
                Some(&SIZE {
                    cx: width,
                    cy: height,
                }),
                Some(dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                }),
                ULW_ALPHA,
            )
            .map_err(Into::into)
        });
        SelectObject(dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::w;
    #[test]
    fn native_surface_has_zero_alpha_center_and_no_input_coverage_there() -> Result<()> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                800,
                400,
                None,
                None,
                None,
                None,
            )?;
            let view = View {
                rect: RECT {
                    left: 100,
                    top: 100,
                    right: 700,
                    bottom: 300,
                },
                bounds: RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                editing: true,
                error: String::new(),
                epoch: 1,
                language: "en".into(),
                hotkey: "Ctrl+F10".into(),
            };
            let layout = Layout::new(&view, 1.0);
            let center = POINT {
                x: 400 - layout.window.left,
                y: 200 - layout.window.top,
            };
            let result = paint_checked(hwnd, &view, 1.0, |pixels, width, _| {
                assert_eq!(pixels[(center.y * width + center.x) as usize], 0);
                assert!(pixels.iter().any(|p| *p >> 24 == 255));
                assert!(pixels.iter().any(|p| (1..255).contains(&(*p >> 24))));
            });
            let region = CreateRectRgn(0, 0, 0, 0);
            let copied = GetWindowRgn(hwnd, region);
            let interior = PtInRegion(region, center.x, center.y).as_bool();
            let quit = PtInRegion(
                region,
                layout.quit.left - layout.window.left + 8,
                layout.quit.top - layout.window.top + 8,
            )
            .as_bool();
            let _ = DeleteObject(region.into());
            let _ = DestroyWindow(hwnd);
            result?;
            assert_ne!(copied, RGN_ERROR);
            assert!(!interior);
            assert!(quit);
        }
        Ok(())
    }
}
