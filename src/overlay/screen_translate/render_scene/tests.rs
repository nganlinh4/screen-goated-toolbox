use super::connectivity::touches;
use super::*;

fn prepared(pixels: PixelRegion) -> PreparedSource {
    PreparedSource {
        pixels,
        source_text: String::new(),
        foreground: String::new(),
        foreground_rgb: None,
        background: None,
    }
}

#[test]
fn touching_rectangles_share_a_component_without_changing_their_bounds() {
    let first = PixelRegion {
        x: 10,
        y: 10,
        width: 20,
        height: 10,
    };
    let second = PixelRegion {
        x: 30,
        y: 15,
        width: 15,
        height: 10,
    };
    let separate = PixelRegion {
        x: 46,
        y: 15,
        width: 10,
        height: 10,
    };
    assert!(touches(first, second));
    assert!(!touches(second, separate));
    assert_eq!(
        first,
        PixelRegion {
            x: 10,
            y: 10,
            width: 20,
            height: 10
        }
    );
}

#[test]
fn dominant_area_keeps_a_mixed_component_vertical() {
    let regions = [
        PixelRegion {
            x: 0,
            y: 0,
            width: 40,
            height: 240,
        },
        PixelRegion {
            x: 45,
            y: 210,
            width: 80,
            height: 20,
        },
    ];
    assert!(dominant_orientation_is_vertical(&regions));
}

#[test]
fn foreground_contrast_is_measured_against_the_merged_background() {
    let background = [109, 7, 18];
    assert!(
        luminance([235, 210, 170]).abs_diff(luminance(background))
            > luminance([147, 16, 42]).abs_diff(luminance(background))
    );
}

#[test]
fn overlapping_detector_rows_become_non_overlapping_text_lanes() {
    let sources = [
        prepared(PixelRegion {
            x: 10,
            y: 10,
            width: 40,
            height: 20,
        }),
        prepared(PixelRegion {
            x: 50,
            y: 10,
            width: 30,
            height: 20,
        }),
        prepared(PixelRegion {
            x: 10,
            y: 25,
            width: 70,
            height: 20,
        }),
    ];
    let references = sources.iter().collect::<Vec<_>>();
    let lanes = source_lanes(
        &[1, 2, 3],
        &references,
        PixelRegion {
            x: 10,
            y: 10,
            width: 70,
            height: 35,
        },
        false,
    );

    assert_eq!(lanes.len(), 2);
    assert_eq!(lanes[0].member_ids, [1, 2]);
    assert_eq!(lanes[1].member_ids, [3]);
    assert!(
        lanes[0].region.y + lanes[0].region.height <= lanes[1].region.y,
        "lane rectangles must not paint over each other"
    );
}

#[test]
fn gaps_in_one_row_remain_distinct_text_lanes() {
    let sources = [
        prepared(PixelRegion {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        }),
        prepared(PixelRegion {
            x: 30,
            y: 0,
            width: 20,
            height: 10,
        }),
    ];
    let references = sources.iter().collect::<Vec<_>>();
    let lanes = source_lanes(
        &[1, 2],
        &references,
        PixelRegion {
            x: 0,
            y: 0,
            width: 50,
            height: 10,
        },
        false,
    );

    assert_eq!(lanes.len(), 2);
    assert_eq!(lanes[0].member_ids, [1]);
    assert_eq!(lanes[1].member_ids, [2]);
}
