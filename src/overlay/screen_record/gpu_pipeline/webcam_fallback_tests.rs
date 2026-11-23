//! A webcam export still renders when no zero-copy ring can be created (WARP,
//! non-DX12 paths, shared NT handles refused): the webcam then decodes on the
//! CPU path, as the primary stream already did. (Forcing every ring off also
//! keeps this debug-build test clear of wgpu's strict barrier assertion on
//! imported shared textures, which release builds do not compile in.)

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use windows::Win32::Graphics::Direct3D11::ID3D11Multithread;
use windows::core::Interface;

use super::decode_thread_cpu::run_decode_thread_cpu;
use super::types::{CpuDecodeContext, DecodeOutput, PipelineConfig};
use super::webcam::FORCE_CPU_PIPELINE;
use super::{CompositorUniforms, GpuCompositor, run_zero_copy_export};
use crate::overlay::screen_record::d3d_interop::create_d3d11_device;
use crate::overlay::screen_record::mf_decode::{self, DxgiDeviceManager};
use crate::overlay::screen_record::mf_encode::{
    EncoderConfig, MfEncoder, VideoCodec, VideoInputSurfaceFormat,
};
use crate::overlay::screen_record::native_export::config::{BakedWebcamFrame, TrimSegment};

const OUT_W: u32 = 320;
const OUT_H: u32 = 240;
const FPS: u32 = 30;
const FRAMES: u32 = 30;

fn write_solid_clip(path: &std::path::Path, width: u32, height: u32, value: u8) {
    let (device, _) = create_d3d11_device().unwrap();
    let multithread: ID3D11Multithread = device.cast().unwrap();
    unsafe {
        let _ = multithread.SetMultithreadProtected(true);
    }
    let manager = DxgiDeviceManager::new(&device).unwrap();
    let (encoder, _) = MfEncoder::new(
        &path.to_string_lossy(),
        EncoderConfig {
            codec: VideoCodec::H264,
            input_surface_format: VideoInputSurfaceFormat::Argb32,
            width,
            height,
            fps_num: FPS,
            fps_den: 1,
            bitrate_kbps: 400,
        },
        &manager,
        None,
    )
    .unwrap();
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|_| [value, value, value, 255])
        .collect();
    let frame_100ns = encoder.frame_duration_100ns();
    for frame in 0..FRAMES {
        encoder
            .write_frame_cpu(&pixels, frame as i64 * frame_100ns, frame_100ns)
            .unwrap();
    }
    encoder.finalize().unwrap();
}

struct TestDir(std::path::PathBuf);

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn test_dir() -> TestDir {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "sgt-webcam-fallback-{}-{}",
        std::process::id(),
        u64::from_ne_bytes(random)
    ));
    std::fs::create_dir_all(&path).unwrap();
    TestDir(path)
}

fn webcam_frame(time: f64) -> BakedWebcamFrame {
    BakedWebcamFrame {
        time,
        visible: true,
        opacity: 1.0,
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 90.0,
        roundness_px: 0.0,
        shadow_px: 0.0,
        mirror: false,
        render_scale: 1.0,
    }
}

/// Reads one BGRA frame of an MP4 through the CPU decode path.
fn read_frame(path: &str, time: f64) -> Vec<u8> {
    let cancel = AtomicBool::new(false);
    let (tx, rx) = mpsc::sync_channel(4);
    let (_recycle_tx, recycle_rx) = mpsc::channel();
    run_decode_thread_cpu(CpuDecodeContext {
        label: "readback",
        cancel_flag: &cancel,
        source_video_path: path,
        source_times: &[time],
        speed_points: &[],
        trim_segments: &[],
        fallback_seek: 0.0,
        framerate: FPS,
        crop_x: 0,
        crop_y: 0,
        source_rect_width: OUT_W,
        source_rect_height: OUT_H,
        output_width: OUT_W,
        output_height: OUT_H,
        active_mask: None,
        tx,
        recycle_rx,
    })
    .unwrap();
    match rx.recv().unwrap() {
        DecodeOutput::Cpu { bgra_video, .. } => bgra_video,
        _ => panic!("expected a CPU frame"),
    }
}

#[test]
#[ignore = "requires Media Foundation H.264 and a GPU adapter"]
fn webcam_export_falls_back_to_cpu_decode_when_rings_are_unavailable() {
    mf_decode::mf_startup().unwrap();
    let dir = test_dir();
    let screen = dir.0.join("screen.mp4");
    let webcam = dir.0.join("webcam.mp4");
    let output = dir.0.join("out.mp4");
    write_solid_clip(&screen, OUT_W, OUT_H, 0);
    write_solid_clip(&webcam, 160, 120, 255);

    let source_times: Vec<f64> = (0..FRAMES).map(|frame| frame as f64 / FPS as f64).collect();
    let config = PipelineConfig {
        source_video_path: screen.to_string_lossy().into_owned(),
        output_path: output.to_string_lossy().into_owned(),
        audio_path: None,
        audio_is_preprocessed: false,
        audio_volume_points: Vec::new(),
        webcam_video_path: Some(webcam.to_string_lossy().into_owned()),
        webcam_offset_sec: 0.0,
        webcam_frames: source_times
            .iter()
            .map(|&time| webcam_frame(time))
            .collect(),
        output_width: OUT_W,
        output_height: OUT_H,
        framerate: FPS,
        bitrate_kbps: 800,
        speed_points: Vec::new(),
        trim_start: 0.0,
        duration: FRAMES as f64 / FPS as f64,
        codec: VideoCodec::H264,
        trim_segments: vec![TrimSegment {
            start_time: 0.0,
            end_time: FRAMES as f64 / FPS as f64,
        }],
        motion_blur_samples: 1,
        blur_zoom_shutter: 0.0,
        blur_pan_shutter: 0.0,
        blur_cursor_shutter: 0.0,
        video_width: OUT_W,
        video_height: OUT_H,
        crop_x: 0,
        crop_y: 0,
        overlay_frames: Vec::new(),
        animated_cursor_slots: Vec::new(),
    };
    let cancel = AtomicBool::new(false);
    let mut compositor =
        GpuCompositor::new(OUT_W, OUT_H, OUT_W, OUT_H, OUT_W, OUT_H, &cancel).unwrap();
    let build_uniforms = |_: f64, _: f64, _: f64, _: f64| {
        let mut uniforms: CompositorUniforms = bytemuck::Zeroable::zeroed();
        uniforms.output_size = [OUT_W as f32, OUT_H as f32];
        uniforms.cursor_pos = [-1000.0, -1000.0];
        uniforms
    };

    FORCE_CPU_PIPELINE.with(|flag| flag.set(true));
    let result = run_zero_copy_export(
        &config,
        &mut compositor,
        &build_uniforms,
        None,
        &cancel,
        &source_times,
    );
    FORCE_CPU_PIPELINE.with(|flag| flag.set(false));
    let result = result.expect("webcam export must not fail without the shared ring");
    assert_eq!(result.frames_encoded, FRAMES);

    let frame = read_frame(&config.output_path, 0.5);
    let luma = |x: u32, y: u32| frame[((y * OUT_W + x) * 4 + 1) as usize];
    let inside = luma(60, 45);
    let outside = luma(250, 200);
    assert!(
        inside > 200 && inside > outside.saturating_add(100),
        "webcam not drawn: inside={inside} outside={outside}"
    );
}
