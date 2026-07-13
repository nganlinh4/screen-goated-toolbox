//! Second-pass visual authorization for coordinate-based pointer actions.

use super::super::vision_reader::Located;
use super::*;
use std::io::Cursor;

const MIN_VERIFY_CONFIDENCE: u64 = 70;

fn crosshair_crop(jpeg: &[u8], loc: &Located) -> Result<Vec<u8>> {
    let source = image::load_from_memory(jpeg)?.to_rgb8();
    let (width, height) = source.dimensions();
    let target_x = (loc.x / 1000.0 * f64::from(width)).round() as i64;
    let target_y = (loc.y / 1000.0 * f64::from(height)).round() as i64;
    let crop_w = (width / 4).max(240).min(width);
    let crop_h = (height / 4).max(180).min(height);
    let left =
        (target_x - i64::from(crop_w) / 2).clamp(0, i64::from(width.saturating_sub(crop_w))) as u32;
    let top = (target_y - i64::from(crop_h) / 2).clamp(0, i64::from(height.saturating_sub(crop_h)))
        as u32;
    let mut crop = image::imageops::crop_imm(&source, left, top, crop_w, crop_h).to_image();
    let cx = (target_x - i64::from(left)).clamp(0, i64::from(crop_w.saturating_sub(1))) as u32;
    let cy = (target_y - i64::from(top)).clamp(0, i64::from(crop_h.saturating_sub(1))) as u32;
    let red = image::Rgb([255, 32, 32]);
    for offset in 4..=14 {
        if let Some(x) = cx.checked_sub(offset) {
            crop.put_pixel(x, cy, red);
        }
        if cx + offset < crop_w {
            crop.put_pixel(cx + offset, cy, red);
        }
        if let Some(y) = cy.checked_sub(offset) {
            crop.put_pixel(cx, y, red);
        }
        if cy + offset < crop_h {
            crop.put_pixel(cx, cy + offset, red);
        }
    }
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(crop).write_to(&mut output, image::ImageFormat::Jpeg)?;
    Ok(output.into_inner())
}

pub(super) fn verify_located(
    fresh_jpeg: &[u8],
    mut loc: Located,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<Located> {
    if std::env::var("CC_VERIFY_LOCATE").as_deref() == Ok("0") {
        return Ok(loc);
    }
    let crop = crosshair_crop(fresh_jpeg, &loc)?;
    let (description, ctx) = (description.to_string(), ctx.to_string());
    let verification = run_cancellable(cancel, move || {
        super::super::vision_reader::verify_target(&crop, &description, &ctx)
    })?;
    if !verification.matches || verification.confidence < MIN_VERIFY_CONFIDENCE {
        anyhow::bail!(
            "visual click verification rejected the point (confidence {}, saw {:?})",
            verification.confidence,
            verification.note
        );
    }
    loc.note = verification.note.or(loc.note);
    Ok(loc)
}

#[cfg(test)]
mod tests {
    use super::crosshair_crop;
    use crate::overlay::computer_control::vision_reader::Located;
    use std::io::Cursor;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn crosshair_crop_handles_screen_edges() {
        let image = image::DynamicImage::new_rgb8(800, 600);
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        for (x, y) in [(0.0, 0.0), (1000.0, 1000.0)] {
            let crop = crosshair_crop(&bytes.get_ref().clone(), &Located { x, y, note: None })
                .expect("edge crop");
            assert!(!crop.is_empty());
        }
    }

    #[test]
    #[ignore = "requires GEMINI_API_KEY and CC_VERIFY_TEST_* inputs"]
    fn live_verification_accepts_annotated_ground_truth() {
        let path = std::env::var("CC_VERIFY_TEST_IMAGE").expect("CC_VERIFY_TEST_IMAGE");
        let target = std::env::var("CC_VERIFY_TEST_TARGET").expect("CC_VERIFY_TEST_TARGET");
        let px = std::env::var("CC_VERIFY_TEST_X_PX")
            .expect("CC_VERIFY_TEST_X_PX")
            .parse::<f64>()
            .unwrap();
        let py = std::env::var("CC_VERIFY_TEST_Y_PX")
            .expect("CC_VERIFY_TEST_Y_PX")
            .parse::<f64>()
            .unwrap();
        let image = image::open(path).unwrap();
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let loc = Located {
            x: px / f64::from(image.width()) * 1000.0,
            y: py / f64::from(image.height()) * 1000.0,
            note: None,
        };
        let verified = super::verify_located(
            bytes.get_ref(),
            loc,
            &target,
            "coordinate verification benchmark",
            &AtomicBool::new(false),
        )
        .expect("ground-truth point should verify");
        assert!(verified.note.is_some());
    }

    #[test]
    #[ignore = "requires GEMINI_API_KEY and CC_VERIFY_TEST_IMAGE/TARGET"]
    fn live_verification_rejects_an_unrelated_point() {
        let path = std::env::var("CC_VERIFY_TEST_IMAGE").expect("CC_VERIFY_TEST_IMAGE");
        let target = std::env::var("CC_VERIFY_TEST_TARGET").expect("CC_VERIFY_TEST_TARGET");
        let image = image::open(path).unwrap();
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let result = super::verify_located(
            bytes.get_ref(),
            Located {
                x: 40.0,
                y: 40.0,
                note: None,
            },
            &target,
            "negative coordinate verification benchmark",
            &AtomicBool::new(false),
        );
        assert!(result.is_err(), "unrelated point must fail closed");
    }
}
