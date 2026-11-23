use super::*;

#[test]
fn a_visual_target_keeps_its_encoded_rectangle_through_signature_validation() {
    let cap = session::Capture {
        rgb: image::RgbImage::new(80, 60),
        origin_x: -100,
        origin_y: -40,
    };
    let requested = View {
        x: -108,
        y: -48,
        w: 96,
        h: 56,
    };
    let (_, shown) = session::encode_view(&cap, requested, 1000, None, None, None).unwrap();
    let location = visual_location(
        &cap,
        shown,
        Located {
            x: 1000.0,
            y: 1000.0,
            note: None,
        },
    );
    assert_eq!(location.screen_point(), (-21, 7));
    assert_ne!(
        location.screen_point(),
        requested.to_screen_px(location.x, location.y)
    );
    assert!(validate_visual_locations(&cap, &[&location]).is_ok());
    let moved = session::Capture {
        rgb: cap.rgb.clone(),
        origin_x: -90,
        origin_y: -40,
    };
    assert!(validate_visual_locations(&moved, &[&location]).is_err());
    assert!(
        require_same_view(
            shown,
            View {
                w: shown.w - 1,
                ..shown
            }
        )
        .is_err()
    );
    let changed = session::Capture {
        rgb: image::RgbImage::from_pixel(80, 60, image::Rgb([255; 3])),
        origin_x: -100,
        origin_y: -40,
    };
    assert!(validate_visual_locations(&changed, &[&location]).is_err());
}
