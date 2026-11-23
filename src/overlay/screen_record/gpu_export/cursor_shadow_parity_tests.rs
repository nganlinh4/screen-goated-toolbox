//! Renders a cursor with its drop shadow through the real export compositor
//! and compares the pixels with what the Canvas 2D preview draws for the same
//! scene. The reference is `parity-fixtures/render-cursor-shadow/golden.json`,
//! captured from Chromium (the preview runs in WebView2) by its generate.py.

use super::cursors::CURSOR_TILE_SIZE;
use super::{CompositorUniforms, GpuCompositor};
use std::sync::atomic::AtomicBool;

const GOLDEN: &str = include_str!("../../../../parity-fixtures/render-cursor-shadow/golden.json");
/// Encoded grey levels. Skia approximates the Gaussian with three box
/// passes, which an exact Gaussian already misses by up to 4.5 levels.
const TOLERANCE: f32 = 6.0;

struct Golden {
    size: u32,
    center: f32,
    scale: f32,
    half_square: f32,
    cases: Vec<serde_json::Value>,
}

fn golden() -> Golden {
    let value: serde_json::Value = serde_json::from_str(GOLDEN).expect("golden parses");
    let frame = &value["frame"];
    let center = frame["cursorCenter"].as_array().expect("cursor center");
    assert_eq!(
        center[0], center[1],
        "the scene keeps the cursor on the diagonal"
    );
    Golden {
        size: frame["size"].as_u64().expect("size") as u32,
        center: center[0].as_f64().expect("center") as f32,
        scale: frame["cursorScale"].as_f64().expect("scale") as f32,
        half_square: frame["halfSquare"].as_f64().expect("half square") as f32,
        cases: value["cases"].as_array().expect("cases").clone(),
    }
}

/// Cursor slot 0 is a 44 x 43 unit sprite stored in a square tile; the white
/// square spans the golden's half-width around the sprite centre.
fn square_cursor_tile(half_square: f32) -> Vec<u8> {
    let side = CURSOR_TILE_SIZE as usize;
    let texels_per_unit = CURSOR_TILE_SIZE as f32 / 44.0;
    let first = ((22.0 - half_square) * texels_per_unit).round() as usize;
    let last = ((22.0 + half_square) * texels_per_unit).round() as usize;
    let mut tile = vec![0u8; side * side * 4];
    for y in first..last {
        for x in first..last {
            tile[(y * side + x) * 4..][..4].copy_from_slice(&[255; 4]);
        }
    }
    tile
}

fn cursor_uniforms(golden: &Golden, strength: f32, opacity: f32) -> CompositorUniforms {
    let size = golden.size as f32;
    let mut uniforms: CompositorUniforms = bytemuck::Zeroable::zeroed();
    uniforms.output_size = [size, size];
    // White background; the video sits outside the frame.
    uniforms.gradient_color1 = [1.0; 4];
    uniforms.gradient_color2 = [1.0; 4];
    uniforms.video_offset = [2.0, 2.0];
    uniforms.video_scale = [0.125, 0.125];
    uniforms.video_size = [size * 0.125; 2];
    uniforms.bg_zoom = 1.0;
    uniforms.bg_anchor_x = 0.5;
    uniforms.bg_anchor_y = 0.5;
    uniforms.cursor_scale = golden.scale;
    uniforms.cursor_opacity = opacity;
    uniforms.cursor_shadow = strength;
    // The shader nudges every cursor by a fixed placement bias, (0.16, -0.16)
    // source units; cancel it so this test pins the shadow, not placement.
    let bias = [0.16 * golden.scale, -0.16 * golden.scale];
    for (axis, nudge) in bias.into_iter().enumerate() {
        let target = (golden.center - nudge) / size;
        uniforms.cursor_pos[axis] =
            (target - uniforms.video_offset[axis]) / uniforms.video_scale[axis];
    }
    uniforms
}

/// Renders one frame and returns its encoded grey levels, row by row.
fn render_grey(golden: &Golden, uniforms: &CompositorUniforms) -> Vec<f32> {
    let cancel = AtomicBool::new(false);
    let mut compositor =
        GpuCompositor::new(golden.size, golden.size, 8, 8, 1, 1, &cancel).expect("compositor");
    compositor.upload_frame(&[255; 8 * 8 * 4]);
    compositor.upload_cursor_slot_rgba(0, &square_cursor_tile(golden.half_square));
    compositor.render_to_output(uniforms, true, None);
    compositor.enqueue_output_readback().unwrap();
    let mut pixels = Vec::new();
    compositor.readback_output(&mut pixels, &cancel).unwrap();
    let (bgra_pixels, _) = pixels.as_chunks::<4>();
    bgra_pixels
        .iter()
        .map(|bgra| {
            assert!(
                bgra[0].abs_diff(bgra[1]) <= 1 && bgra[1].abs_diff(bgra[2]) <= 1,
                "grey"
            );
            f32::from(bgra[1])
        })
        .collect()
}

fn values(line: &serde_json::Value) -> Vec<f32> {
    line["values"]
        .as_array()
        .expect("values")
        .iter()
        .map(|v| v.as_f64().expect("grey") as f32)
        .collect()
}

#[test]
#[ignore = "renders on the GPU"]
fn cursor_shadow_matches_the_canvas_preview() {
    let golden = golden();
    let size = golden.size as usize;
    let mut failures = Vec::new();
    for case in &golden.cases {
        let name = case["name"].as_str().expect("name");
        let strength = case["strength"].as_f64().expect("strength") as f32;
        let opacity = case["opacity"].as_f64().expect("opacity") as f32;
        let grey = render_grey(&golden, &cursor_uniforms(&golden, strength, opacity));
        let field = |line: &str, key: &str| case[line][key].as_u64().expect(key) as usize;
        // (name, preview values, first pixel index, index step)
        let lines = [
            (
                "row",
                values(&case["row"]),
                field("row", "y") * size + field("row", "x0"),
                1,
            ),
            (
                "column",
                values(&case["column"]),
                field("column", "y0") * size + field("column", "x"),
                size,
            ),
            (
                "diagonal",
                values(&case["diagonal"]),
                field("diagonal", "start") * (size + 1),
                size + 1,
            ),
        ];
        for (line, preview, first, stride) in &lines {
            let index = |i: usize| first + i * stride;
            let (worst, at) = preview
                .iter()
                .enumerate()
                .map(|(i, &want)| ((grey[index(i)] - want).abs(), i))
                .fold(
                    (0.0, 0),
                    |best, next| if next.0 > best.0 { next } else { best },
                );
            println!("{name} {line}: worst {worst} at offset {at}");
            if worst > TOLERANCE {
                failures.push(format!(
                    "{name} {line} offset {at}: export {} vs preview {}",
                    grey[index(at)],
                    preview[at]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
