use super::*;
use sgt_screen_text_detector_protocol::stream::{LayoutKind, LayoutRegion, Region};

#[test]
fn geometry_maps_back_to_original_capture() {
    let payload = Payload {
        bytes: vec![1],
        width: 100,
        height: 50,
        source_width: 300,
        source_height: 200,
    };
    let mut event = Event::Geometry {
        width: 100,
        height: 50,
        regions: vec![Region {
            id: 1,
            quad: [[10.0, 5.0], [20.0, 5.0], [20.0, 10.0], [10.0, 10.0]],
            confidence: 1.0,
        }],
        layout: vec![LayoutRegion {
            bounds: [10.0, 5.0, 20.0, 10.0],
            kind: LayoutKind::Text,
        }],
    };
    payload.map_event(&mut event);
    let Event::Geometry {
        width,
        height,
        regions,
        layout,
    } = event
    else {
        panic!("geometry expected");
    };
    assert_eq!((width, height), (300, 200));
    assert_eq!(regions[0].quad[0], [30.0, 20.0]);
    assert_eq!(layout[0].bounds, [30.0, 20.0, 60.0, 40.0]);
    let candidate = super::super::incremental::candidate(&regions[0], width, height).unwrap();
    assert_eq!(candidate.bounds.image_size(), (300, 200));
    assert_eq!(
        candidate.bounds.pixels(),
        super::super::geometry::PixelRegion {
            x: 30,
            y: 20,
            width: 30,
            height: 20
        }
    );
}

#[test]
fn resized_quads_reach_original_pixels_without_a_normalized_roundtrip() {
    let payload = Payload {
        bytes: vec![1],
        width: 3840,
        height: 120,
        source_width: 7680,
        source_height: 240,
    };
    let mut event = Event::Geometry {
        width: 3840,
        height: 120,
        regions: vec![Region {
            id: 1,
            quad: [
                [501.625, 15.125],
                [550.375, 15.125],
                [550.375, 24.75],
                [501.625, 24.75],
            ],
            confidence: 1.0,
        }],
        layout: vec![],
    };
    payload.map_event(&mut event);
    let Event::Geometry {
        width,
        height,
        regions,
        ..
    } = event
    else {
        panic!("geometry expected");
    };
    let candidate = super::super::incremental::candidate(&regions[0], width, height).unwrap();
    assert_eq!(
        candidate.bounds.pixels(),
        super::super::geometry::PixelRegion {
            x: 1003,
            y: 30,
            width: 98,
            height: 20
        }
    );
}

#[test]
fn dimensions_and_frame_admission_obey_worker_limits() {
    assert!(dimensions(0, 1).is_err());
    for (width, height) in [(8193, 1000), (8192, 8192), (u32::MAX, u32::MAX)] {
        let (w, h) = dimensions(width, height).unwrap();
        assert!(w > 0 && h > 0 && w <= MAX_SIDE && h <= MAX_SIDE);
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS);
    }
    assert!(validate_detector_input(&[1], 8192, 8192).is_err());
    assert!(validate_detector_input(&[1], 8192, 1).is_ok());
    assert!(validate_detector_input(&[], 1, 1).is_err());
}

#[test]
fn cancelled_preparation_returns_without_encoding() {
    let image = RgbaImage::new(20, 20);
    assert!(prepare_png(&image, &AtomicBool::new(true)).is_err());
}

fn noise(width: u32, height: u32) -> RgbaImage {
    let mut seed = 0x1234_5678u32;
    let mut bytes = vec![0; width as usize * height as usize * 4];
    for byte in &mut bytes {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        *byte = seed as u8;
    }
    RgbaImage::from_raw(width, height, bytes).unwrap()
}

#[test]
fn a_capture_over_the_retired_16_mib_frame_stays_lossless() {
    let payload = prepare_png(&noise(2100, 2100), &AtomicBool::new(false)).unwrap();
    assert_eq!((payload.width, payload.height), (2100, 2100));
    assert!(payload.bytes.len() > 16 * 1024 * 1024);
    assert!(payload.bytes.starts_with(b"\x89PNG"));
    validate_detector_input(&payload.bytes, payload.width, payload.height).unwrap();
}

#[test]
fn a_capture_over_the_frame_limit_falls_back_to_a_bounded_jpeg() {
    let limit = 120_000;
    let payload = prepare_within(&noise(256, 256), &AtomicBool::new(false), limit).unwrap();
    assert_eq!((payload.width, payload.height), (256, 256));
    assert!(payload.bytes.len() <= limit);
    assert!(payload.bytes.starts_with(&[0xff, 0xd8]));
}
