//! Second-pass visual authorization for coordinate-based pointer actions.

use super::super::vision_contract::{MIN_VERIFICATION_CONFIDENCE, crosshair_crop};
use super::super::vision_reader::Located;
use super::*;

pub(super) fn verify_located(
    fresh_jpeg: &[u8],
    mut loc: Located,
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<Located> {
    if super::harness_options::skip_locate_verification_requested() {
        return Ok(loc);
    }
    let crop = crosshair_crop(fresh_jpeg, loc.x, loc.y)?;
    let (description, ctx) = (description.to_string(), ctx.to_string());
    let verification = run_cancellable(cancel, move || {
        super::super::vision_reader::verify_target(&crop, &description, &ctx)
    })?;
    if !verification.matches || verification.confidence < MIN_VERIFICATION_CONFIDENCE {
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
            let crop = crosshair_crop(bytes.get_ref(), x, y).expect("edge crop");
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
