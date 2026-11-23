use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WS_POPUP,
};
use windows::core::w;

mod dib;

const CF_UNICODETEXT: u32 = 13;

struct ClipboardOwner {
    hwnd: HWND,
    owned: bool,
}

impl ClipboardOwner {
    fn new(hwnd: HWND) -> windows::core::Result<Self> {
        if !hwnd.is_invalid() {
            return Ok(Self { hwnd, owned: false });
        }
        // EmptyClipboard requires a real window to own the published data.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )?
        };
        Ok(Self { hwnd, owned: true })
    }
}

impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        if self.owned {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn set_global_clipboard_data(format: u32, data: &[u8]) -> bool {
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, data.len()) else {
            return false;
        };
        let pointer = GlobalLock(memory) as *mut u8;
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), pointer, data.len());
        let _ = GlobalUnlock(memory);
        if SetClipboardData(format, Some(HANDLE(memory.0))).is_ok() {
            true
        } else {
            let _ = GlobalFree(Some(memory));
            false
        }
    }
}

/// Whether Windows Clipboard History and cloud sync may retain a publication.
#[derive(Clone, Copy)]
enum Retention {
    /// An ordinary user-facing copy: request Clipboard History capture.
    History,
    /// Content the user did not copy themselves: exclude it from history and
    /// cloud sync while it remains pasteable.
    Private,
}

fn set_dword_format(name: windows::core::PCWSTR, value: u32) -> bool {
    let format = unsafe { RegisterClipboardFormatW(name) };
    format != 0 && set_global_clipboard_data(format, &value.to_ne_bytes())
}

fn mark_retention(retention: Retention) -> bool {
    match retention {
        Retention::History => set_dword_format(w!("CanIncludeInClipboardHistory"), 1),
        Retention::Private => {
            set_dword_format(w!("CanIncludeInClipboardHistory"), 0)
                && set_dword_format(w!("CanUploadToCloudClipboard"), 0)
        }
    }
}

// --- CLIPBOARD SUPPORT ---
pub fn copy_to_clipboard(text: &str, hwnd: HWND) -> bool {
    copy_text_with_sequence(text, hwnd).is_some()
}

/// Return the sequence captured while our published content is still locked.
pub(crate) fn copy_text_with_sequence(text: &str, hwnd: HWND) -> Option<u32> {
    publish_text(text, hwnd, Retention::History)
}

/// Like [`copy_text_with_sequence`], but kept out of Clipboard History and
/// cloud clipboard sync.
pub(crate) fn copy_private_text_with_sequence(text: &str, hwnd: HWND) -> Option<u32> {
    publish_text(text, hwnd, Retention::Private)
}

fn publish_text(text: &str, hwnd: HWND, retention: Retention) -> Option<u32> {
    let owner = ClipboardOwner::new(hwnd).ok()?;
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe {
        std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), std::mem::size_of_val(&*utf16))
    };
    unsafe {
        // Retry loop to handle temporary clipboard locks
        for attempt in 0..5 {
            if OpenClipboard(Some(owner.hwnd)).is_ok() {
                let emptied = EmptyClipboard().is_ok();
                let text_published = emptied && set_global_clipboard_data(CF_UNICODETEXT, bytes);
                let retention_marked = text_published && mark_retention(retention);
                let sequence = GetClipboardSequenceNumber();
                let closed = CloseClipboard().is_ok();
                return (text_published && retention_marked && sequence != 0 && closed)
                    .then_some(sequence);
            }

            // If failed and not last attempt, wait before retrying
            if attempt < 4 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            } else {
                eprintln!("Failed to copy to clipboard after 5 attempts");
            }
        }
    }
    None
}

/// Publish a decoded image with an acknowledged clipboard write.
pub fn try_copy_image_to_clipboard(image_bytes: &[u8], hwnd: HWND) -> anyhow::Result<()> {
    copy_image_with_sequence(image_bytes, hwnd).map(|_| ())
}

pub(crate) fn copy_image_with_sequence(image_bytes: &[u8], hwnd: HWND) -> anyhow::Result<u32> {
    use anyhow::{Context, ensure};
    // RGB DIB is broadly pasteable; PNG retains the exact encoded image for
    // consumers that support the registered format. Allocate before opening.
    let image = image::load_from_memory(image_bytes)?.to_rgb8();
    let mut bmp = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bmp, image::ImageFormat::Bmp)?;
    let bmp = bmp.into_inner();
    ensure!(
        bmp.starts_with(b"BM") && bmp.len() > 14,
        "invalid clipboard bitmap"
    );
    let owner = ClipboardOwner::new(hwnd)?;
    unsafe {
        for attempt in 0..5 {
            if OpenClipboard(Some(owner.hwnd)).is_ok() {
                let result = (|| -> anyhow::Result<u32> {
                    EmptyClipboard().context("clipboard could not be cleared")?;
                    ensure!(
                        set_global_clipboard_data(8, &bmp[14..]),
                        "clipboard bitmap publication failed"
                    );
                    if image_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                        let png_format = RegisterClipboardFormatW(w!("PNG"));
                        if png_format != 0 {
                            set_global_clipboard_data(png_format, image_bytes);
                        }
                    }
                    mark_retention(Retention::History);
                    let sequence = GetClipboardSequenceNumber();
                    ensure!(sequence != 0, "clipboard sequence unavailable");
                    Ok(sequence)
                })();
                let close_result = CloseClipboard().context("clipboard could not be closed");
                return result.and_then(|sequence| close_result.map(|()| sequence));
            }
            if attempt < 4 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
    anyhow::bail!("clipboard remained locked")
}

/// Read image bytes from clipboard (returns PNG-encoded bytes)
/// Returns None if no image is available in clipboard
pub fn get_clipboard_image_bytes() -> Option<Vec<u8>> {
    unsafe {
        for _attempt in 0..5 {
            if OpenClipboard(None).is_ok() {
                let png_format = RegisterClipboardFormatW(w!("PNG"));
                let image = dib::decode_preferred(png_format, |format, is_png| {
                    if IsClipboardFormatAvailable(format).is_err() {
                        return None;
                    }
                    let handle = HGLOBAL(GetClipboardData(format).ok()?.0);
                    let size = GlobalSize(handle);
                    if size == 0 || size > crate::image_decode::MAX_ENCODED_BYTES {
                        return None;
                    }
                    let pointer = GlobalLock(handle).cast::<u8>();
                    if pointer.is_null() {
                        return None;
                    }
                    let bytes = std::slice::from_raw_parts(pointer, size);
                    let result = if is_png {
                        dib::decode_png(bytes)
                    } else {
                        dib::decode(bytes)
                    };
                    let _ = GlobalUnlock(handle);
                    result
                });
                let _ = CloseClipboard();
                let image = image?;
                let mut png = std::io::Cursor::new(Vec::new());
                image.write_to(&mut png, image::ImageFormat::Png).ok()?;
                return Some(png.into_inner());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_owner_supplies_and_releases_a_window_for_null_handles() -> anyhow::Result<()> {
        use windows::Win32::UI::WindowsAndMessaging::IsWindow;
        let owner = ClipboardOwner::new(HWND::default())?;
        let hwnd = owner.hwnd;
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
        let borrowed = ClipboardOwner::new(hwnd)?;
        drop(borrowed);
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
        drop(owner);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
        Ok(())
    }

    #[test]
    fn foreign_dib_headers_cannot_claim_more_pixels_than_their_block() {
        // A 40-byte CF_DIB header followed by one pixel, claiming a huge bitmap.
        let claimed = |width: i32, height: i32| {
            let mut block = vec![0_u8; 40];
            block[0..4].copy_from_slice(&40_u32.to_le_bytes());
            block[4..8].copy_from_slice(&width.to_le_bytes());
            block[8..12].copy_from_slice(&height.to_le_bytes());
            block[12..14].copy_from_slice(&1_u16.to_le_bytes());
            block[14..16].copy_from_slice(&32_u16.to_le_bytes());
            block.extend_from_slice(&[1, 2, 3, 4]);
            block
        };
        assert!(dib::decode(&claimed(1, 1)).is_some());
        for (width, height) in [(46_341, 46_341), (20_000, 20_000), (20_000, -20_000)] {
            assert!(
                dib::decode(&claimed(width, height)).is_none(),
                "{width}x{height}"
            );
        }
    }

    #[test]
    #[ignore = "replaces the Windows clipboard with generated text"]
    fn null_owner_text_publication_keeps_matching_sequence() -> anyhow::Result<()> {
        let sequence = copy_text_with_sequence("clipboard publication test", HWND::default())
            .ok_or_else(|| anyhow::anyhow!("text publication failed"))?;
        assert_eq!(unsafe { GetClipboardSequenceNumber() }, sequence);
        unsafe {
            OpenClipboard(None)?;
            let result = (|| -> anyhow::Result<()> {
                let data = GetClipboardData(CF_UNICODETEXT)?;
                let memory = HGLOBAL(data.0);
                let ptr = GlobalLock(memory).cast::<u16>();
                anyhow::ensure!(!ptr.is_null(), "clipboard text could not be locked");
                let len = GlobalSize(memory) / 2;
                let units = std::slice::from_raw_parts(ptr, len);
                let end = units.iter().position(|unit| *unit == 0).unwrap_or(len);
                let text = String::from_utf16_lossy(&units[..end]);
                let _ = GlobalUnlock(memory);
                assert_eq!(text, "clipboard publication test");
                Ok(())
            })();
            let _ = CloseClipboard();
            result?;
        }
        Ok(())
    }

    #[test]
    #[ignore = "replaces the Windows clipboard with a generated image"]
    fn copied_image_round_trips_pixels_through_native_clipboard() -> anyhow::Result<()> {
        use windows::Win32::UI::WindowsAndMessaging::*;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Clipboard image verification"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )?
        };
        let result = (|| -> anyhow::Result<()> {
            let image = image::RgbImage::from_fn(7, 5, |x, y| {
                image::Rgb([x as u8 * 30, y as u8 * 40, 123])
            });
            let mut png = std::io::Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png)?;
            try_copy_image_to_clipboard(png.get_ref(), hwnd)?;
            let readback = get_clipboard_image_bytes()
                .ok_or_else(|| anyhow::anyhow!("missing clipboard bitmap"))?;
            assert_eq!(image::load_from_memory(&readback)?.to_rgb8(), image);
            let sequence = unsafe { GetClipboardSequenceNumber() };
            assert!(try_copy_image_to_clipboard(b"invalid image", hwnd).is_err());
            assert_eq!(unsafe { GetClipboardSequenceNumber() }, sequence);
            Ok(())
        })();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        result
    }
}
