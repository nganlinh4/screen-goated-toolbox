//! Real-clipboard check of the paste refusal. It replaces the user's
//! clipboard while it runs, so it is ignored by default and restores the
//! prior text or image afterwards.

use super::*;
use windows::Win32::Foundation::HWND;

fn one_pixel_png() -> Vec<u8> {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(1, 1, image::Rgb([200, 40, 40]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    png.into_inner()
}

#[test]
#[ignore = "mutates the real clipboard; run only with the user's approval"]
fn live_paste_refuses_an_image_on_the_real_clipboard_without_touching_it() {
    let saved_text = super::super::clipboard::get_text_for_query().ok();
    let saved_image = crate::overlay::utils::get_clipboard_image_bytes();

    crate::overlay::utils::try_copy_image_to_clipboard(&one_pixel_png(), HWND::default())
        .expect("publish a test image");
    let before = super::super::clipboard::sequence();
    let refusal = irreplaceable_clipboard_refusal();
    let after = super::super::clipboard::sequence();

    super::super::clipboard::stage_text("plain text paste check").expect("publish text");
    let text_refusal = irreplaceable_clipboard_refusal();

    match (saved_image, saved_text) {
        (Some(image), _) => {
            let _ = crate::overlay::utils::try_copy_image_to_clipboard(&image, HWND::default());
        }
        (None, Some(text)) if !text.is_empty() => {
            let _ = crate::overlay::utils::copy_to_clipboard(&text, HWND::default());
        }
        _ => {}
    }

    let refusal = refusal.expect("an image on the clipboard must refuse the paste");
    assert_eq!(refusal["code"], "ERR_CLIPBOARD_HOLDS_NONTEXT");
    assert_eq!(refusal["executed"], false);
    assert_eq!(before, after, "the refusal must not replace the clipboard");
    assert!(text_refusal.is_none(), "plain text has a text equivalent");
}
