//! Reuse the shell's exact bundled face without requiring a system installation.
use anyhow::{Result, ensure};
use std::sync::OnceLock;
use windows::{Win32::Graphics::Gdi::*, core::w};

pub(super) fn create(scale: f32) -> Result<HFONT> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let registered = REGISTERED.get_or_init(|| unsafe {
        let data = crate::assets::GOOGLE_SANS_FLEX_EGUI;
        let mut count = 0;
        // Windows retains the private font until process exit; bytes are also static.
        let handle = AddFontMemResourceEx(
            data.as_ptr().cast(),
            data.len() as u32,
            None,
            &raw mut count,
        );
        !handle.is_invalid() && count > 0
    });
    ensure!(*registered, "could not register the app's UI font");
    let font = unsafe {
        CreateFontW(
            -(13.0 * scale).round() as i32,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Google Sans Flex"),
        )
    };
    ensure!(!font.is_invalid(), "could not create the app's UI font");
    Ok(font)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_face_is_the_bundled_app_font() -> Result<()> {
        unsafe {
            let font = create(1.0)?;
            let dc = CreateCompatibleDC(None);
            let old = SelectObject(dc, font.into());
            let mut name = [0u16; 64];
            let count = GetTextFaceW(dc, Some(&mut name));
            SelectObject(dc, old);
            let _ = DeleteObject(font.into());
            let _ = DeleteDC(dc);
            ensure!(count > 0, "font name missing");
            assert_eq!(
                String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap()]),
                "Google Sans Flex"
            );
        }
        Ok(())
    }
}
