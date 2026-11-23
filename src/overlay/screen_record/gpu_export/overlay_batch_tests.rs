//! Text overlays larger than the overlay vertex buffer are drawn in batches
//! through the real export compositor instead of being dropped for the frame,
//! and the webcam pass still runs on such a frame.

use super::GpuCompositor;
use crate::overlay::screen_record::native_export::config::{BakedWebcamFrame, OverlayQuad};
use std::sync::atomic::AtomicBool;

const SIZE: u32 = 64;
/// Whole quads that fit the 1 MiB overlay vertex buffer.
const QUADS_PER_BATCH: usize =
    1024 * 1024 / (6 * std::mem::size_of::<super::setup::OverlayVertex>());

fn quad(x: f32, y: f32, size: f32, alpha: f32) -> OverlayQuad {
    OverlayQuad {
        x,
        y,
        w: size,
        h: size,
        u: 0.0,
        v: 0.0,
        uw: 1.0,
        vh: 1.0,
        alpha,
    }
}

/// Gives the compositor a white 1x1 webcam frame.
fn load_white_webcam(compositor: &mut GpuCompositor) {
    compositor.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: compositor.webcam_overlay.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255; 4],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    compositor.webcam_overlay.mark_has_frame();
}

#[test]
#[ignore = "renders on the GPU"]
fn quads_past_the_vertex_buffer_are_drawn_in_later_batches() {
    let cancel = AtomicBool::new(false);
    let mut compositor =
        GpuCompositor::new(SIZE, SIZE, 8, 8, 1, 1, &cancel).expect("export compositor");
    compositor.upload_atlas(&[255; 4], 1, 1).unwrap(); // one white texel
    let mut uniforms: super::CompositorUniforms = bytemuck::Zeroable::zeroed();
    uniforms.output_size = [SIZE as f32, SIZE as f32];
    uniforms.cursor_pos = [-100.0, -100.0];
    compositor.render_to_output(&uniforms, true, None); // a black frame

    // A full first batch of invisible quads, then one visible quad that only
    // a second batch can draw.
    let mut quads = vec![quad(0.0, 0.0, 1.0, 0.0); QUADS_PER_BATCH];
    quads.push(quad(8.0, 8.0, 16.0, 1.0));
    load_white_webcam(&mut compositor);
    let webcam = BakedWebcamFrame {
        time: 0.0,
        visible: true,
        opacity: 1.0,
        x: 40.0,
        y: 40.0,
        width: 16.0,
        height: 16.0,
        roundness_px: 0.0,
        shadow_px: 0.0,
        mirror: false,
        render_scale: 1.0,
    };
    compositor.render_post_overlays(Some(&webcam), &quads);

    compositor.enqueue_output_readback().unwrap();
    let mut pixels = Vec::new();
    compositor.readback_output(&mut pixels, &cancel).unwrap();
    let grey = |x: u32, y: u32| pixels[((y * SIZE + x) * 4 + 1) as usize];
    assert!(grey(16, 16) > 200, "last quad drawn, got {}", grey(16, 16));
    assert!(grey(48, 48) > 200, "webcam drawn, got {}", grey(48, 48));
    assert!(grey(40, 16) < 50, "elsewhere the frame stays black");
}
