use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::gpu_export::{CompositorUniformParams, GpuCompositor, create_uniforms};
use crate::overlay::screen_record::engine::{ENCODER_ACTIVE, IS_RECORDING};

use super::{EXPORT_ACTIVE, EXPORT_GPU_WARMED};

const EXPORT_WARMUP_IDLE_DELAY: Duration = Duration::from_secs(15);
const EXPORT_WARMUP_IDLE_POLL: Duration = Duration::from_millis(500);

pub fn warm_up_export_pipeline_when_idle() {
    let mut idle_since: Option<Instant> = None;

    loop {
        if EXPORT_GPU_WARMED.load(Ordering::SeqCst) {
            println!("[Export][Warmup] already complete, idle scheduler exiting");
            return;
        }

        let recording_active =
            IS_RECORDING.load(Ordering::SeqCst) || ENCODER_ACTIVE.load(Ordering::SeqCst);
        if recording_active || EXPORT_ACTIVE.load(Ordering::SeqCst) {
            idle_since = None;
            std::thread::sleep(EXPORT_WARMUP_IDLE_POLL);
            continue;
        }

        let idle_start = idle_since.get_or_insert_with(Instant::now);
        if idle_start.elapsed() < EXPORT_WARMUP_IDLE_DELAY {
            std::thread::sleep(EXPORT_WARMUP_IDLE_POLL);
            continue;
        }

        warm_up_export_pipeline();
        return;
    }
}

pub fn warm_up_export_pipeline() {
    if EXPORT_ACTIVE.load(Ordering::SeqCst) {
        println!("[Export][Warmup] export active, skipping warm-up");
        return;
    }
    if IS_RECORDING.load(Ordering::SeqCst) || ENCODER_ACTIVE.load(Ordering::SeqCst) {
        println!("[Export][Warmup] recording active, deferring warm-up");
        return;
    }
    if EXPORT_GPU_WARMED.swap(true, Ordering::SeqCst) {
        println!("[Export][Warmup] already started/skipped");
        return;
    }
    if IS_RECORDING.load(Ordering::SeqCst) || ENCODER_ACTIVE.load(Ordering::SeqCst) {
        EXPORT_GPU_WARMED.store(false, Ordering::SeqCst);
        println!("[Export][Warmup] recording started during warm-up launch, deferring");
        return;
    }

    let warmup_start = Instant::now();
    let warm_w = 1920u32;
    let warm_h = 1080u32;
    println!(
        "[Export][Warmup] starting GPU warm-up {}x{}",
        warm_w, warm_h
    );

    match GpuCompositor::new(warm_w, warm_h, warm_w, warm_h, warm_w, warm_h) {
        Ok(compositor) => {
            let _ = compositor.init_cursor_texture_fast(&[0]);

            let blank_frame = vec![0u8; (warm_w * warm_h * 4) as usize];
            compositor.upload_frame(&blank_frame);

            let uniforms = create_uniforms(CompositorUniformParams {
                video_offset: (0.0, 0.0),
                video_scale: (1.0, 1.0),
                output_size: (warm_w as f32, warm_h as f32),
                video_size: (warm_w as f32, warm_h as f32),
                border_radius: 0.0,
                shadow_offset: 0.0,
                shadow_blur: 0.0,
                shadow_opacity: 0.0,
                gradient_color1: [0.0, 0.0, 0.0, 1.0],
                gradient_color2: [0.0, 0.0, 0.0, 1.0],
                gradient_color3: [0.0, 0.0, 0.0, 0.0],
                gradient_color4: [0.0, 0.0, 0.0, 0.0],
                gradient_color5: [0.0, 0.0, 0.0, 0.0],
                bg_params1: [0.0, 0.0, 0.0, 0.0],
                bg_params2: [0.0, 0.0, 0.0, 0.0],
                bg_params3: [0.0, 0.0, 0.0, 0.0],
                bg_params4: [0.0, 0.0, 0.0, 0.0],
                bg_params5: [0.0, 0.0, 0.0, 0.0],
                bg_params6: [0.0, 0.0, 0.0, 0.0],
                time: 0.0,
                render_mode: 0.0,
                cursor_pos: (-1.0, -1.0),
                cursor_scale: 0.0,
                cursor_opacity: 0.0,
                cursor_type_id: 0.0,
                cursor_rotation: 0.0,
                cursor_shadow: 0.0,
                use_background_texture: false,
                bg_zoom: 1.0,
                bg_anchor: (0.5, 0.5),
                background_style: 0.0,
                bg_tex_w: 0.0,
                bg_tex_h: 0.0,
            });

            let mut warm_compositor = compositor;
            let _ = warm_compositor.render_frame(&uniforms);
            println!(
                "[Export][Warmup] GPU export pipeline warmed up in {:.2}s",
                warmup_start.elapsed().as_secs_f64()
            );
        }
        Err(err) => {
            eprintln!("[Export][Warmup] GPU warm-up failed: {}", err);
        }
    }
}
