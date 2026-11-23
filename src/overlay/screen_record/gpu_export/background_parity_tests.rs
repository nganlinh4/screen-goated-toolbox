//! Renders built-in background families through the real export compositor
//! and compares pixels with what the Canvas 2D preview draws: Canvas
//! gradients interpolate encoded sRGB stops, and a radial overlay's alpha
//! falls off linearly to zero at its radius before source-over compositing.
//! The video frame carries the preview's faint 1px outline.

use super::{CompositorUniforms, GpuCompositor};
use std::sync::atomic::AtomicBool;

const SIZE: u32 = 64;

fn scene_uniforms() -> CompositorUniforms {
    let mut uniforms: CompositorUniforms = bytemuck::Zeroable::zeroed();
    uniforms.output_size = [SIZE as f32, SIZE as f32];
    // A small video in the bottom-right corner, away from every sample.
    uniforms.video_offset = [0.85, 0.85];
    uniforms.video_scale = [0.125, 0.125];
    uniforms.video_size = [8.0, 8.0];
    uniforms.render_mode = 1.0;
    uniforms.cursor_pos = [-100.0, -100.0];
    uniforms.bg_zoom = 1.0;
    uniforms.bg_anchor_x = 0.5;
    uniforms.bg_anchor_y = 0.5;
    uniforms
}

/// Renders one frame and returns the grey level (0-255) at each sample.
fn render_grey(uniforms: &CompositorUniforms, samples: &[(u32, u32)]) -> Vec<f32> {
    let cancel = AtomicBool::new(false);
    let mut compositor =
        GpuCompositor::new(SIZE, SIZE, 8, 8, 1, 1, &cancel).expect("export compositor");
    compositor.upload_frame(&[255; 8 * 8 * 4]); // a white video frame
    compositor.render_to_output(uniforms, true, None);
    compositor.enqueue_output_readback().unwrap();
    let mut pixels = Vec::new();
    compositor.readback_output(&mut pixels, &cancel).unwrap();
    samples
        .iter()
        .map(|&(x, y)| {
            let at = ((y * SIZE + x) * 4) as usize;
            let (b, g, r) = (pixels[at], pixels[at + 1], pixels[at + 2]);
            assert!(b.abs_diff(g) <= 1 && g.abs_diff(r) <= 1, "grey expected");
            g as f32
        })
        .collect()
}

fn pixel_uv(x: u32, y: u32) -> (f32, f32) {
    (
        (x as f32 + 0.5) / SIZE as f32,
        (y as f32 + 0.5) / SIZE as f32,
    )
}

#[test]
#[ignore = "renders on the GPU"]
fn linear_gradient_interpolates_like_the_canvas_preview() {
    let mut uniforms = scene_uniforms();
    uniforms.bg_style = 0.0;
    uniforms.bg_params1 = [1.0, 0.0, 0.0, 0.0]; // vertical axis
    uniforms.gradient_color1 = [0.0, 0.0, 0.0, 1.0];
    uniforms.gradient_color2 = [1.0, 1.0, 1.0, 1.0];
    let samples = [(2, 16), (2, 31), (2, 47)];
    let rendered = render_grey(&uniforms, &samples);
    for (&(x, y), got) in samples.iter().zip(rendered) {
        // Canvas: black-to-white stops mix in encoded sRGB.
        let expected = pixel_uv(x, y).1 * 255.0;
        assert!(
            (got - expected).abs() <= 2.0,
            "row {y}: export {got} vs preview {expected:.1}"
        );
    }
}

#[test]
#[ignore = "renders on the GPU"]
fn stacked_radial_overlay_fades_like_the_canvas_preview() {
    let mut uniforms = scene_uniforms();
    uniforms.bg_style = 3.0;
    uniforms.gradient_color1 = [0.0, 0.0, 0.0, 1.0];
    uniforms.gradient_color2 = [0.0, 0.0, 0.0, 1.0];
    uniforms.gradient_color3 = [0.0, 0.0, 0.0, 1.0];
    uniforms.gradient_color4 = [1.0, 1.0, 1.0, 1.0];
    uniforms.bg_params2 = [0.5, 0.5, 0.5, 0.8]; // center, radius, opacity
    let samples = [(40, 32), (48, 32), (36, 36)];
    let rendered = render_grey(&uniforms, &samples);
    for (&(x, y), got) in samples.iter().zip(rendered) {
        let (u, v) = pixel_uv(x, y);
        let distance = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
        let alpha = 0.8 * (1.0 - (distance / 0.5).clamp(0.0, 1.0));
        // Source-over of white at `alpha` on black, in encoded sRGB.
        let expected = alpha * 255.0;
        assert!(
            (got - expected).abs() <= 2.0,
            "pixel ({x},{y}): export {got} vs preview {expected:.1}"
        );
    }
}

#[test]
#[ignore = "renders on the GPU"]
fn video_frame_keeps_the_preview_outline() {
    let mut uniforms = scene_uniforms();
    uniforms.video_offset = [0.75, 0.75]; // pixels 48..56
    uniforms.border_radius = 0.0;
    uniforms.bg_style = 0.0;
    uniforms.gradient_color1 = [1.0, 1.0, 1.0, 1.0];
    uniforms.gradient_color2 = [1.0, 1.0, 1.0, 1.0];
    let rendered = render_grey(&uniforms, &[(48, 52), (52, 52), (40, 52)]);
    // Canvas strokes rgba(0,0,0,0.1) 1px wide on the frame edge and clips it
    // to the frame: the edge pixel is half covered, so white drops by 5%.
    let edge = 255.0 * 0.95;
    assert!(
        (rendered[0] - edge).abs() <= 2.0,
        "edge {} vs preview {edge:.1}",
        rendered[0]
    );
    assert_eq!(rendered[1], 255.0, "the outline stays on the edge");
    assert_eq!(rendered[2], 255.0, "the background is untouched");
}
