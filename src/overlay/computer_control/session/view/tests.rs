use super::*;
use crate::overlay::computer_control::session::{
    encode_view, target_fingerprint_matches, target_region_fingerprint, view_fingerprint,
};

fn view(x: i32, y: i32, w: i32, h: i32) -> View {
    View { x, y, w, h }
}

fn capture() -> Capture {
    Capture {
        rgb: image::RgbImage::new(80, 60),
        origin_x: -100,
        origin_y: -40,
    }
}

#[test]
fn clipping_trims_each_edge_without_shifting_the_requested_extent() {
    let bounds = view(-100, -40, 80, 60);
    for (requested, expected) in [
        (view(-108, -48, 96, 56), view(-100, -40, 80, 48)),
        (view(-90, -30, 100, 100), view(-90, -30, 70, 50)),
        (view(-108, -30, 28, 20), view(-100, -30, 20, 20)),
        (view(-90, -48, 20, 28), view(-90, -40, 20, 20)),
    ] {
        assert_eq!(requested.clamped_to(bounds), expected);
        assert_eq!(expected.clamped_to(bounds), expected);
    }
}

#[test]
fn degenerate_frame_fallback_is_shared_and_idempotent() {
    let bounds = view(-100, -40, 80, 60);
    for requested in [
        view(500, 500, 20, 20),
        view(-90, -30, 0, 20),
        view(-90, -30, 7, 20),
    ] {
        assert_eq!(requested.clamped_to(bounds), bounds);
        assert_eq!(requested.clamped_to(bounds).clamped_to(bounds), bounds);
    }
}

#[test]
fn screen_pixels_round_trip_through_normalized_coordinates() {
    for width in [1, 2, 80, 1920] {
        let region = view(-1700, -300, width, 600);
        for x in [region.x, region.x + width / 2, region.x + width - 1] {
            for y in [region.y, region.y + 300, region.y + 599] {
                let (nx, ny) = region.screen_to_norm(x, y);
                assert_eq!(region.to_screen_px(nx, ny), (x, y));
            }
        }
    }
}

#[test]
fn extreme_rectangle_edges_do_not_overflow() {
    let requested = view(i32::MIN, -10, i32::MAX, 30);
    assert_eq!(
        requested.intersection(view(-100, 0, 200, 10)),
        Some(view(-100, 0, 99, 10))
    );
    let bounds = view(i32::MAX - 5, 0, 4, 10);
    assert_eq!(
        view(i32::MAX - 10, 0, 100, 10).intersection(bounds),
        Some(bounds)
    );
}

#[test]
fn encoded_pixels_and_reported_screen_rectangle_agree() {
    let mut cap = capture();
    for y in 48..60 {
        for x in 0..80 {
            cap.rgb.put_pixel(x, y, image::Rgb([255; 3]));
        }
    }
    let (jpeg, shown) = encode_view(&cap, view(-108, -48, 96, 56), 1000, None, None, None).unwrap();
    assert_eq!(shown, view(-100, -40, 80, 48));
    let decoded = image::load_from_memory(&jpeg).unwrap().to_rgb8();
    assert_eq!(decoded.dimensions(), (80, 48));
    assert!(
        decoded
            .pixels()
            .all(|pixel| pixel.0.iter().all(|value| *value < 4))
    );
    assert!(
        view_fingerprint(&cap, shown)
            .iter()
            .all(|value| *value == 0)
    );
}

#[test]
fn target_signatures_clip_the_requested_box_without_pulling_in_adjacent_pixels() {
    let mut cap = capture();
    for y in 0..60 {
        for x in 10..80 {
            cap.rgb.put_pixel(x, y, image::Rgb([255; 3]));
        }
    }
    let signature = target_region_fingerprint(&cap, -100, -10, 10);
    assert_eq!(signature.len(), 24 * 24 * 3);
    assert!(signature.iter().all(|value| *value == 0));
    assert!(target_region_fingerprint(&cap, 500, 500, 10).is_empty());
    assert!(!target_fingerprint_matches(&[], &[]));
}

#[test]
fn empty_or_unrepresentable_captures_return_errors() {
    let mut cap = capture();
    cap.rgb = image::RgbImage::new(0, 0);
    assert!(encode_view(&cap, view(0, 0, 10, 10), 1000, None, None, None).is_err());
    cap = capture();
    cap.origin_x = i32::MAX;
    assert!(cap.bounds().is_err());
}
