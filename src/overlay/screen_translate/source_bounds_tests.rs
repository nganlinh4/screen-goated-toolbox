use super::super::{contract::DetectedTextRegion, incremental, render_scene, runtime, units};
use super::*;
use sgt_screen_text_detector_protocol::stream::{LayoutKind, LayoutRegion, Region};

fn region(left: f32, right: f32) -> Region {
    Region {
        id: 1,
        quad: [[left, 30.25], [right, 30.25], [right, 49.5], [left, 49.5]],
        confidence: 1.0,
    }
}

#[test]
fn wide_quads_keep_outward_raster_edges_and_derive_prompt_boxes_separately() {
    let candidate = incremental::candidate(&region(1003.25, 1100.75), 7680, 240).unwrap();
    assert_eq!(
        candidate.bounds.pixels(),
        PixelRegion {
            x: 1003,
            y: 30,
            width: 98,
            height: 20
        }
    );
    let prompt_pixels =
        super::super::geometry::normalized_region(candidate.bounds.prompt(), 7680, 240);
    assert_ne!(candidate.bounds.pixels(), prompt_pixels);
    let shifted = incremental::candidate(&region(1004.25, 1101.75), 7680, 240).unwrap();
    assert_eq!(candidate.bounds.prompt(), shifted.bounds.prompt());
    assert_ne!(candidate.bounds.pixels(), shifted.bounds.pixels());
}

#[test]
fn response_order_uses_source_pixels_when_prompt_boxes_are_identical() {
    let mut right = incremental::candidate(&region(1004.25, 1101.75), 7680, 240).unwrap();
    let mut left = incremental::candidate(&region(1003.25, 1100.75), 7680, 240).unwrap();
    left.id = 2;
    right.source_text = "right".into();
    left.source_text = "left".into();
    right.source_alternatives = vec![right.source_text.clone()];
    left.source_alternatives = vec![left.source_text.clone()];
    assert_eq!(left.bounds.prompt(), right.bounds.prompt());
    let document = super::super::contract::parse_response(
        r#"{"translations":{"0":"translated right","1":"translated left"}}"#,
        &[right, left],
    )
    .unwrap();
    assert_eq!(
        document
            .regions
            .iter()
            .map(|region| region.id)
            .collect::<Vec<_>>(),
        [2, 1]
    );
}

#[test]
fn pixel_union_and_axis_rescaling_do_not_round_through_prompt_coordinates() {
    let first = SourceBounds::from_pixels(
        PixelRegion {
            x: 1003,
            y: 30,
            width: 98,
            height: 20,
        },
        7680,
        240,
    )
    .unwrap();
    let second = SourceBounds::from_pixels(
        PixelRegion {
            x: 1102,
            y: 31,
            width: 9,
            height: 22,
        },
        7680,
        240,
    )
    .unwrap();
    let joined = SourceBounds::union([first, second]).unwrap();
    assert_eq!(
        joined.pixels(),
        PixelRegion {
            x: 1003,
            y: 30,
            width: 108,
            height: 23
        }
    );
    assert_eq!(
        joined.rescale(15360, 120).unwrap().pixels(),
        PixelRegion {
            x: 2006,
            y: 15,
            width: 216,
            height: 12
        }
    );
    assert!(SourceBounds::union([first, first.rescale(3840, 120).unwrap()]).is_err());
    assert!(SourceBounds::union([]).is_err());
}

#[test]
fn source_bounds_reject_empty_nonfinite_and_overflowing_geometry() {
    let rect = PixelRegion {
        x: 9,
        y: 9,
        width: 2,
        height: 2,
    };
    assert!(SourceBounds::from_pixels(rect, 10, 10).is_err());
    assert!(
        SourceBounds::from_pixels(
            PixelRegion {
                x: u32::MAX,
                ..rect
            },
            u32::MAX,
            10
        )
        .is_err()
    );
    assert!(SourceBounds::from_pixels(PixelRegion { width: 0, ..rect }, 20, 20).is_err());
    assert!(SourceBounds::from_quad(&[[f32::NAN, 0.0]; 4], 10, 10).is_err());
    assert!(SourceBounds::from_quad(&[[2.0, 2.0]; 4], 10, 10).is_err());
    assert!(SourceBounds::from_normalized([0, 0, 10, 10].into(), 0, 10).is_err());
}

#[test]
fn wide_geometry_reaches_unit_planning_processing_and_scene_without_quantization() {
    const CHILD: &str = "SGT_TEST_SOURCE_GEOMETRY";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "overlay::screen_translate::source_bounds::tests::wide_geometry_reaches_unit_planning_processing_and_scene_without_quantization", "--nocapture"])
            .env(CHILD, "1").status().unwrap();
        assert!(status.success());
        return;
    }
    let image = image::RgbaImage::from_pixel(7680, 240, image::Rgba([255; 4]));
    let mut second = region(1003.25, 1110.75);
    second.id = 2;
    for point in &mut second.quad {
        point[1] += 24.0;
    }
    let mut sources: Vec<DetectedTextRegion> = vec![
        incremental::candidate(&region(1003.25, 1100.75), 7680, 240).unwrap(),
        incremental::candidate(&second, 7680, 240).unwrap(),
    ];
    sources[0].source_text = "sample".into();
    sources[1].source_text = "next".into();
    let expected = sources[0].bounds.pixels();
    let union = PixelRegion {
        x: 1003,
        y: 30,
        width: 108,
        height: 44,
    };
    let layout = [LayoutRegion {
        bounds: [1000.0, 25.0, 1120.0, 80.0],
        kind: LayoutKind::Text,
    }];
    let plan = units::Plan::new(&image, &mut sources, &layout).unwrap();
    assert_eq!(plan.units.len(), 1);
    assert_eq!(plan.units[0].members, [1, 2]);
    assert_eq!(plan.candidates[0].bounds.pixels(), union);
    assert_eq!(
        super::super::geometry::processing_cells(&sources),
        vec![[1003, 30, 98, 20], [1003, 54, 108, 20]]
    );
    let (job, _) = runtime::begin_subtitle_job();
    let capture = crate::overlay::selection::CapturedRegion {
        image,
        left: -1920,
        top: 50,
        width: 7680,
        height: 240,
    };
    let scene = render_scene::prepare_scene(job, &capture, &sources, Some(&plan.units)).unwrap();
    assert_eq!(scene.sources[&1].pixels, expected);
    assert_eq!(scene.blocks[0].layout, union);
    runtime::cancel_job(job);
}
